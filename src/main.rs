use brightness_device::BrightnessDevice;
use logind_session::LogindSessionProxy;
use notify::{event::ModifyKind, EventKind, Watcher};
use std::sync::atomic::AtomicU64;
use std::time::Duration;
use std::{
    collections::{HashMap, HashSet},
    future, io,
    path::PathBuf,
    sync::{atomic::Ordering, Arc},
};
use theme::watch_theme;
use tokio::{
    io::{unix::AsyncFd, Interest},
    sync::RwLock,
    task,
};
use tokio_stream::StreamExt;
use zbus::{
    names::{MemberName, UniqueName, WellKnownName},
    zvariant::ObjectPath,
    Connection, MatchRule, MessageStream, SignalContext,
};

mod brightness_device;
mod logind_session;
mod theme;

// Use seperate HasDisplayBrightness, or -1?
// Is it fair to assume a display device will notify on change?
// TODO: notifications; statusnotifierwatcher, media keybindings
// Scale brightness to 0 to 100? Or something else? Float?

pub static ID_COUNTER: AtomicU64 = AtomicU64::new(0);
const GEOCLUE_AGENT: Option<&'static str> = option_env!("GEOCLUE_AGENT");

static DBUS_NAME: &str = "com.system76.CosmicSettingsDaemon";
static DBUS_PATH: &str = "/com/system76/CosmicSettingsDaemon";

struct SettingsDaemon {
    logind_session: Option<LogindSessionProxy<'static>>,
    display_brightness_device: Option<BrightnessDevice>,
    watched_configs: Arc<
        RwLock<HashMap<(String, u64), (Connection, ObjectPath<'static>, WellKnownName<'static>)>>,
    >,
    watched_states: Arc<
        RwLock<HashMap<(String, u64), (Connection, ObjectPath<'static>, WellKnownName<'static>)>>,
    >,
}

#[derive(Debug)]
enum Config {
    Config,
    State,
}

impl Config {
    fn new_config() -> Self {
        Self::Config
    }

    fn new_state() -> Self {
        Self::State
    }
}

#[zbus::dbus_interface(name = "com.system76.CosmicSettingsDaemon.Config")]
impl Config {
    #[dbus_interface(signal)]
    async fn changed(ctxt: &SignalContext<'_>, id: String, key: String) -> zbus::Result<()>;
}

impl Config {
    fn path(&self, id: &str, version: u64) -> ObjectPath<'static> {
        let cfg_type = if matches!(self, Config::State) {
            "State"
        } else {
            "Config"
        };
        // convert id to path
        let id = id.replace('.', "/");

        ObjectPath::try_from(format!(
            "/com/system76/CosmicSettingsDaemon/{cfg_type}/{id}/V{version}",
        ))
        .unwrap_or_else(|_| {
            let next_id = ID_COUNTER.fetch_add(1, Ordering::SeqCst);

            ObjectPath::try_from(format!(
                "/com/system76/CosmicSettingsDaemon/{cfg_type}/C{next_id}/V{version}",
            ))
            .unwrap()
        })
    }

    fn name(&self, id: &str, version: u64) -> WellKnownName<'static> {
        let cfg_type = if matches!(self, Config::State) {
            "State"
        } else {
            "Config"
        };
        if let Ok(name) = WellKnownName::try_from(format!(
            "com.system76.CosmicSettingsDaemon.{cfg_type}.{id}.V{version}",
        )) {
            name
        } else {
            let next_id = ID_COUNTER.fetch_add(1, Ordering::SeqCst);
            WellKnownName::try_from(format!(
                "com.system76.CosmicSettingsDaemon.{cfg_type}.C{next_id}.V{version}",
            ))
            .unwrap()
        }
    }
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
                _ = brightness_device
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
    async fn set_keyboard_brightness(&self, _value: i32) {}

    async fn increase_display_brightness(
        &self,
        #[zbus(signal_context)] ctxt: zbus::SignalContext<'_>,
    ) {
        let value = self.display_brightness().await;
        if let Some(brightness_device) = self.display_brightness_device.as_ref() {
            let step = brightness_device.brightness_step() as i32;
            self.set_display_brightness((value + step).max(0)).await;
            _ = self.display_brightness_changed(&ctxt).await;
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
            _ = self.display_brightness_changed(&ctxt).await;
        }
    }

    async fn increase_keyboard_brightness(&self) {}

    async fn decrease_keyboard_brightness(&self) {}

    async fn watch_config(
        &mut self,
        id: &str,
        version: u64,
    ) -> zbus::fdo::Result<(ObjectPath<'static>, WellKnownName<'static>)> {
        // create a new config, return the path and add it to our hashmap
        Self::watch_config_inner(self, Config::new_config(), id, version).await
    }

    async fn watch_state(
        &mut self,
        id: &str,
        version: u64,
    ) -> zbus::fdo::Result<(ObjectPath<'static>, WellKnownName<'static>)> {
        Self::watch_config_inner(self, Config::new_state(), id, version).await
    }
}

impl SettingsDaemon {
    async fn watch_config_inner(
        &mut self,
        config: Config,
        id: &str,
        version: u64,
    ) -> zbus::fdo::Result<(ObjectPath<'static>, WellKnownName<'static>)> {
        let configs = match config {
            Config::Config => &self.watched_configs,
            Config::State => &self.watched_states,
        };
        if let Some((_, path, name)) = configs.read().await.get(&(id.to_string(), version)) {
            return Ok((path.to_owned(), name.to_owned()));
        }
        let path = config.path(id, version);
        let name = config.name(id, version);
        let conn = zbus::ConnectionBuilder::session()?
            .name(name.as_str())?
            .serve_at(path.to_owned(), config)?
            .build()
            .await?;

        configs.write().await.insert(
            (id.to_owned(), version),
            (conn, path.to_owned(), name.to_owned()),
        );
        Ok((path.to_owned(), name.to_owned()))
    }
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
                for evt in socket.get_inner().iter() {
                    eprintln!("{:?}: {:?}", evt.event_type(), evt.device());
                    match evt.event_type() {
                        udev::EventType::Add => {
                            backlights.insert(evt.syspath().to_owned(), evt.device());
                            let device = choose_best_backlight(&backlights).await;
                            interface.get_mut().await.display_brightness_device = device;
                            _ = interface
                                .get()
                                .await
                                .display_brightness_changed(&ctxt)
                                .await;
                        }
                        udev::EventType::Remove => {
                            backlights.remove(evt.syspath());
                            let device = choose_best_backlight(&backlights).await;
                            interface.get_mut().await.display_brightness_device = device;
                            _ = interface
                                .get()
                                .await
                                .display_brightness_changed(&ctxt)
                                .await;
                        }
                        udev::EventType::Change => {
                            _ = interface
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

#[derive(Debug)]
pub enum Change {
    Config(String, String, u64),
    State(String, String, u64),
    Ping(String, u64),
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> zbus::Result<()> {
    std::process::Command::new(GEOCLUE_AGENT.unwrap_or("/usr/libexec/geoclue-2.0/demos/agent"))
        .spawn()?;
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
            let xdg_config = dirs::config_dir()
                .map(|x| x.join("cosmic"))
                .or_else(|| dirs::home_dir().map(|p| p.join(".config/cosmic")));
            let xdg_state = dirs::state_dir()
                .map(|x| x.join("cosmic"))
                .or_else(|| dirs::home_dir().map(|p| p.join(".local/state/cosmic")));
            let xdg_config_clone = xdg_config.clone();
            let xdg_state_clone = xdg_state.clone();
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut watcher =
                notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                    if let Ok(event) = res {
                        match &event.kind {
                            EventKind::Access(_) | EventKind::Modify(ModifyKind::Metadata(_)) => {
                                // Data not mutated
                                return;
                            }
                            _ => {}
                        }
                        let msgs: Vec<_> = event
                            .paths
                            .into_iter()
                            .filter_map(|path| {
                                if !path.is_file() {
                                    return None;
                                }
                                let (path, is_state) = if let Some(path) = xdg_config_clone
                                    .as_ref()
                                    .and_then(|prefix| path.strip_prefix(prefix).ok())
                                {
                                    (path, false)
                                } else if let Some(path) = xdg_state_clone
                                    .as_ref()
                                    .and_then(|prefix| path.strip_prefix(prefix).ok())
                                {
                                    (path, true)
                                } else {
                                    return None;
                                };
                                // really only care about keys
                                if path.starts_with(".atomicwrite") {
                                    return None;
                                }

                                let key = path.file_name().map(|f| f.to_string_lossy())?;
                                let version = path.parent().and_then(|parent_dir| {
                                    parent_dir
                                        .file_name()
                                        .and_then(|f| f.to_str())
                                        .and_then(|f| {
                                            f.strip_prefix('v').and_then(|f| f.parse::<u64>().ok())
                                        })
                                })?;

                                let id = path.parent().and_then(|parent_dir| {
                                    parent_dir.parent().map(|f| f.to_string_lossy())
                                })?;

                                if is_state {
                                    Some(Change::State(id.into_owned(), key.into_owned(), version))
                                } else {
                                    Some(Change::Config(id.into_owned(), key.into_owned(), version))
                                }
                            })
                            .collect();
                        if let Err(err) = tx.send(msgs) {
                            eprintln!("Failed to send config change: {}", err);
                        }
                    }
                })
                .expect("Failed to create notify watcher");

            if let Some(xdg_config) = xdg_config {
                if let Err(err) = watcher.watch(&xdg_config, notify::RecursiveMode::Recursive) {
                    eprintln!("Failed to watch xdg config dir: {}", err);
                }
            }
            if let Some(xdg_state) = xdg_state {
                if let Err(err) = watcher.watch(&xdg_state, notify::RecursiveMode::Recursive) {
                    eprintln!("Failed to watch xdg state dir: {}", err);
                }
            }
            let watched_configs = Arc::new(RwLock::new(HashMap::new()));
            let watched_states = Arc::new(RwLock::new(HashMap::new()));
            let settings_daemon = SettingsDaemon {
                logind_session: logind_session.ok(),
                display_brightness_device,
                watched_configs: watched_configs.clone(),
                watched_states: watched_states.clone(),
            };

            let connection = zbus::ConnectionBuilder::session()?
                .name(DBUS_NAME)?
                .serve_at(DBUS_PATH, settings_daemon)?
                .build()
                .await?;

            let conn_clone = connection.clone();

            task::spawn_local(async move {
                backlight_monitor_task(backlights, conn_clone).await;
            });
            let (theme_tx, mut theme_rx) = tokio::sync::mpsc::channel(10);
            task::spawn_local(async move {
                let mut sleep = Duration::from_millis(100);

                loop {
                    if let Err(err) = watch_theme(&mut theme_rx).await {
                        eprintln!(
                            "Failed to watch theme {err:?}. Will try again in {}s",
                            sleep.as_secs()
                        );
                    }
                    tokio::time::sleep(sleep).await;
                    sleep = sleep.saturating_mul(2);
                }
            });
            let conn_clone = connection.clone();
            task::spawn_local(async move {
                if let Err(err) =
                    watch_config_message_stream(conn_clone, watched_configs, watched_states).await
                {
                    eprintln!("Failed to watch config message stream: {}", err);
                }
            });

            let conn_clone = connection.clone();
            task::spawn_local(async move {
                while let Some(changes) = rx.recv().await {
                    let Ok(settings_daemon) = conn_clone
                        .object_server()
                        .interface::<_, SettingsDaemon>(DBUS_PATH)
                        .await
                    else {
                        continue;
                    };
                    let settings_daemon = settings_daemon.get().await;
                    for c in changes {
                        if let Change::Config(id, key, version) = c {
                            if id.as_str() == cosmic_theme::THEME_MODE_ID {
                                if let Err(err) = theme_tx.send(key.clone()).await {
                                    eprintln!("Failed to send theme mode update {err:?}");
                                }
                            }
                            let read_guard = settings_daemon.watched_configs.read().await;
                            let Some((conn, path, _)) = read_guard.get(&(id.to_string(), version))
                            else {
                                continue;
                            };
                            let Ok(config) =
                                conn.object_server().interface::<_, Config>(path).await
                            else {
                                continue;
                            };

                            if let Err(err) = Config::changed(
                                config.signal_context(),
                                id.to_string(),
                                key.to_string(),
                            )
                            .await
                            {
                                eprintln!("Failed to send config changed signal: {}", err);
                            }
                        } else if let Change::State(id, key, version) = c {
                            let read_guard = settings_daemon.watched_states.read().await;
                            let Some((conn, path, _)) = read_guard.get(&(id.to_string(), version))
                            else {
                                continue;
                            };

                            let Ok(state) = conn.object_server().interface::<_, Config>(path).await
                            else {
                                continue;
                            };

                            if let Err(err) = Config::changed(
                                state.signal_context(),
                                id.to_string(),
                                key.to_string(),
                            )
                            .await
                            {
                                eprintln!("Failed to send state changed signal: {}", err);
                            }
                        }
                    }
                }
            });

            future::pending::<()>().await;

            Ok(())
        })
        .await
}

async fn watch_config_message_stream(
    conn: Connection,
    watched_configs: Arc<
        RwLock<HashMap<(String, u64), (Connection, ObjectPath<'static>, WellKnownName<'static>)>>,
    >,
    watched_states: Arc<
        RwLock<HashMap<(String, u64), (Connection, ObjectPath<'static>, WellKnownName<'static>)>>,
    >,
) -> zbus::Result<()> {
    let config_rule = MatchRule::builder()
        .msg_type(zbus::MessageType::MethodCall)
        .member("WatchConfig")?
        .interface("com.system76.CosmicSettingsDaemon")?
        .build();
    let config_stream = MessageStream::for_match_rule(config_rule, &conn, Some(100)).await?;

    let mut watched_config_names: HashMap<(String, u64), HashSet<UniqueName<'static>>> =
        HashMap::new();

    let state_rule = MatchRule::builder()
        .msg_type(zbus::MessageType::MethodCall)
        .member("WatchState")?
        .interface("com.system76.CosmicSettingsDaemon")?
        .build();
    let state_stream = MessageStream::for_match_rule(state_rule, &conn, Some(100)).await?;

    let mut watched_state_names: HashMap<(String, u64), HashSet<UniqueName<'static>>> =
        HashMap::new();

    let name_changed_rule = MatchRule::builder()
        .msg_type(zbus::MessageType::Signal)
        .sender("org.freedesktop.DBus")?
        .member("NameOwnerChanged")?
        .interface("org.freedesktop.DBus")?
        .arg(2, "")? // new owner is empty
        .build();

    let name_changed_stream =
        MessageStream::for_match_rule(name_changed_rule, &conn, Some(100)).await?;

    let mut rx = name_changed_stream.merge(config_stream).merge(state_stream);

    while let Some(msg) = rx.try_next().await? {
        if msg.member() == Some(MemberName::from_static_str_unchecked("NameOwnerChanged")) {
            let Ok((name, old_owner, _)) = msg.body::<(String, String, String)>() else {
                continue;
            };
            if name != old_owner {
                continue;
            }
            let unique_name = UniqueName::from_str_unchecked(&old_owner).to_owned();
            for ((k, v), is_config) in watched_config_names
                .iter_mut()
                .map(|a| (a, true))
                .chain(watched_state_names.iter_mut().map(|a| (a, false)))
                .filter(|((_, v), _)| v.contains(&unique_name))
            {
                v.remove(&unique_name);
                if v.is_empty() {
                    let mut write_guard = if is_config {
                        watched_configs.write().await
                    } else {
                        watched_states.write().await
                    };
                    write_guard.retain(|(id, version), (_, _, _)| &k.0 != id || &k.1 != version);
                }
            }
            watched_config_names.retain(|_, v| !v.is_empty());
            watched_state_names.retain(|_, v| !v.is_empty());
        } else if msg.member() == Some(MemberName::from_static_str_unchecked("WatchConfig")) {
            let Ok(header) = msg.header() else {
                continue;
            };
            let Ok(Some(sender)) = header.sender() else {
                continue;
            };
            let Ok((id, version)) = msg.body::<(String, u64)>() else {
                continue;
            };

            let name_set = watched_config_names
                .entry((id.clone(), version))
                .or_default();
            name_set.insert(sender.to_owned());
        } else if msg.member() == Some(MemberName::from_static_str_unchecked("WatchState")) {
            let Ok(header) = msg.header() else {
                continue;
            };
            let Ok(Some(sender)) = header.sender() else {
                continue;
            };
            let Ok((id, version)) = msg.body::<(String, u64)>() else {
                continue;
            };

            let name_set = watched_state_names
                .entry((id.clone(), version))
                .or_default();
            name_set.insert(sender.to_owned());
        }
    }

    Ok(())
}
