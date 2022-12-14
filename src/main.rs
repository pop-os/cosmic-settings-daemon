use std::{collections::HashMap, future, io, path::PathBuf};
use tokio::{
    io::{unix::AsyncFd, Interest},
    task,
};

mod brightness_device;
use brightness_device::BrightnessDevice;
mod logind_session;
use logind_session::LogindSessionProxy;

// Use seperate HasDisplayBrightness, or -1?
// Is it fair to assume a display device will notify on change?
// TODO: notifications; statusnotifierwatcher, media keybindings
// Scale brightness to 0 to 100? Or something else? Float?

static DBUS_NAME: &str = "com.system76.CosmicSettingsDaemon";
static DBUS_PATH: &str = "/com/system76/CosmicSettingsDaemon";

struct SettingsDaemon {
    logind_session: Option<LogindSessionProxy<'static>>,
    display_brightness_device: Option<BrightnessDevice>,
}

#[zbus::dbus_interface(name = "com.system76.CosmicSettingsDaemon")]
impl SettingsDaemon {
    #[dbus_interface(property)]
    async fn display_brightness(&self) -> i32 {
        if let Some(brightness_device) = self.display_brightness_device.as_ref() {
            // XXX error
            brightness_device
                .brightness()
                .await
                .ok()
                .map(|x| x as i32)
                .unwrap_or(-1)
        } else {
            -1
        }
    }

    #[dbus_interface(property)]
    async fn set_display_brightness(&self, value: i32) {
        if let Some(logind_session) = self.logind_session.as_ref() {
            if let Some(brightness_device) = self.display_brightness_device.as_ref() {
                brightness_device
                    .set_brightness(logind_session, value as u32)
                    .await;
            }
        }
    }

    #[dbus_interface(property)]
    async fn keyboard_brightness(&self) -> i32 {
        -1
    }

    #[dbus_interface(property)]
    async fn set_keyboard_brightness(&self, value: i32) {}

    async fn increase_display_brightness(
        &self,
        #[zbus(signal_context)] ctxt: zbus::SignalContext<'_>,
    ) {
        let value = self.display_brightness().await;
        if let Some(brightness_device) = self.display_brightness_device.as_ref() {
            let step = brightness_device.brightness_step() as i32;
            self.set_display_brightness((value + step).max(0)).await;
            self.display_brightness_changed(&ctxt).await;
        }
    }

    async fn decrease_display_brightness(
        &self,

        #[zbus(signal_context)] ctxt: zbus::SignalContext<'_>,
    ) {
        let value = self.display_brightness().await;
        if let Some(brightness_device) = self.display_brightness_device.as_ref() {
            let step = brightness_device.brightness_step() as i32;
            self.set_display_brightness((value - step).max(0)).await;
            self.display_brightness_changed(&ctxt).await;
        }
    }

    async fn increase_keyboard_brightness(&self) {}

    async fn decrease_keyboard_brightness(&self) {}
}

fn backlight_enumerate() -> io::Result<Vec<udev::Device>> {
    let mut enumerator = udev::Enumerator::new()?;
    enumerator.match_subsystem("backlight")?;
    Ok(enumerator.scan_devices()?.collect())
}

fn backlight_monitor() -> io::Result<AsyncFd<udev::MonitorSocket>> {
    let socket = udev::MonitorBuilder::new()?
        .match_subsystem("backlight")?
        .listen()?;
    AsyncFd::with_interest(socket, Interest::READABLE | Interest::WRITABLE)
}

// Choose backlight with most "precision". This is what `light` does.
async fn choose_best_backlight(
    udev_devices: &HashMap<PathBuf, udev::Device>,
) -> Option<BrightnessDevice> {
    let mut best_backlight = None;
    let mut best_max_brightness = 0;
    for device in udev_devices.values() {
        if let Some(sysname) = device.sysname().to_str() {
            match BrightnessDevice::new("backlight", sysname.to_owned()).await {
                Ok(brightness_device) => {
                    if brightness_device.max_brightness() > best_max_brightness {
                        best_max_brightness = brightness_device.max_brightness();
                        best_backlight = Some(brightness_device);
                    }
                }
                Err(err) => eprintln!("Failed to read max brightness: {}", err),
            }
        }
    }
    best_backlight
}

async fn backlight_monitor_task(
    mut backlights: HashMap<PathBuf, udev::Device>,
    connection: zbus::Connection,
) {
    let interface = connection
        .object_server()
        .interface::<_, SettingsDaemon>(DBUS_PATH)
        .await
        .unwrap();

    let ctxt = zbus::SignalContext::new(&connection, DBUS_PATH).unwrap();

    match backlight_monitor() {
        Ok(mut socket) => {
            loop {
                let mut socket = socket.writable_mut().await.unwrap(); // XXX
                for evt in socket.get_inner_mut() {
                    eprintln!("{:?}: {:?}", evt.event_type(), evt.device());
                    match evt.event_type() {
                        udev::EventType::Add => {
                            backlights.insert(evt.syspath().to_owned(), evt.device());
                            let device = choose_best_backlight(&backlights).await;
                            interface.get_mut().await.display_brightness_device = device;
                            interface
                                .get()
                                .await
                                .display_brightness_changed(&ctxt)
                                .await;
                        }
                        udev::EventType::Remove => {
                            backlights.remove(evt.syspath());
                            let device = choose_best_backlight(&backlights).await;
                            interface.get_mut().await.display_brightness_device = device;
                            interface
                                .get()
                                .await
                                .display_brightness_changed(&ctxt)
                                .await;
                        }
                        udev::EventType::Change => {
                            interface
                                .get()
                                .await
                                .display_brightness_changed(&ctxt)
                                .await;
                        }
                        _ => {}
                    }
                }
                socket.clear_ready();
            }
        }
        Err(err) => eprintln!("Error creating udev backlight monitor: {}", err),
    };
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> zbus::Result<()> {
    task::LocalSet::new()
        .run_until(async {
            let backlights = match backlight_enumerate() {
                Ok(backlights) => backlights,
                Err(err) => {
                    eprintln!("Failed to enumerate backlights: {}", err);
                    Vec::new()
                }
            };
            let backlights: HashMap<_, _> = backlights
                .into_iter()
                .map(|i| (i.syspath().to_owned(), i))
                .collect();
            let display_brightness_device = choose_best_backlight(&backlights).await;

            let logind_session = async {
                let connection = zbus::Connection::system().await?;
                LogindSessionProxy::builder(&connection).build().await
            }
            .await;

            let settings_daemon = SettingsDaemon {
                logind_session: logind_session.ok(),
                display_brightness_device,
            };

            let connection = zbus::ConnectionBuilder::session()?
                .name(DBUS_NAME)?
                .serve_at(DBUS_PATH, settings_daemon)?
                .build()
                .await?;

            task::spawn_local(async move {
                backlight_monitor_task(backlights, connection).await;
            });

            future::pending::<()>().await;

            Ok(())
        })
        .await
}
