use futures::FutureExt;
use notify_rust::Notification;
use std::path::Path;
use std::pin::Pin;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::error::TryRecvError;
use tokio_stream::StreamExt;
use upower_dbus::{BatteryLevel, UPowerProxy};

pub type OnBatteryFn = Box<dyn Fn(bool) -> Pin<Box<dyn Future<Output = bool>>>>;

// TODO: Add config parameter for changing the preferred sound theme.

/// If a battery is detected, begin watching for on_battery events from upower.
async fn watch_battery_plug_events(
    conn: &zbus::Connection,
    upower: &UPowerProxy<'static>,
) -> (bool, OnBatteryFn) {
    let mut line_power_device = None;
    let line_power_device_path = match upower.enumerate_devices().await {
        Ok(devices) => devices.into_iter().find(|device_path| {
            device_path
                .rsplit_once('/')
                .is_some_and(|(_, name)| name.starts_with("line_power"))
        }),
        _ => None,
    };

    if let Some(path) = line_power_device_path
        && let Ok(device) = upower_dbus::DeviceProxy::new(conn, path).await
        && let Ok(online) = device.online().await
    {
        line_power_device = Some((device, !online))
    }

    match line_power_device {
        Some((device, is_on_battery)) => {
            let mut on_battery_plugged_stream = device.receive_online_changed().await;
            let (battery_plugged_tx, battery_plugged_rx) =
                tokio::sync::watch::channel(is_on_battery);
            tokio::task::spawn_local(async move {
                let mut debounce_task: Option<tokio::task::JoinHandle<bool>> = None;
                loop {
                    let Some(property) = on_battery_plugged_stream.next().await else {
                        continue;
                    };

                    let Ok(is_online) = property.get().await else {
                        continue;
                    };

                    // Cancel pending message-sending task if it is still waiting.
                    if let Some(task) = debounce_task.take() {
                        task.abort();

                        // Break from this task if there was an error sending a message.
                        if let Ok(true) = task.await {
                            break;
                        }
                    }

                    // Delay message-sending with a task that waits 500ms before sending.
                    let battery_plugged_tx = battery_plugged_tx.clone();
                    debounce_task = Some(tokio::task::spawn_local(async move {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        battery_plugged_tx.send(!is_online).is_err()
                    }));

                    continue;
                }
            });

            let watch_fn: OnBatteryFn = Box::new(move |on_battery: bool| {
                let mut battery_plugged_rx = battery_plugged_rx.clone();
                Box::pin(async move {
                    _ = battery_plugged_rx.wait_for(|&v| v != on_battery).await;
                    *battery_plugged_rx.borrow_and_update()
                })
            });

            (is_on_battery, watch_fn)
        }

        None => (
            false,
            Box::new(|_on_battery: bool| futures::future::pending::<bool>().boxed()),
        ),
    }
}

pub async fn low_power_monitor() {
    let Some(conn) = crate::utils::zbus_system_connection().await else {
        return;
    };

    let Ok(upower) = upower_dbus::UPowerProxy::new(&conn).await else {
        return;
    };

    let Ok(device) = upower.get_display_device().await else {
        return;
    };

    let mut current_battery = BatteryLevel::Full;
    let mut last_critical_notification = Instant::now();
    let mut last_low_notification = last_critical_notification;
    let mut percent_changed_stream = device.receive_percentage_changed().await;
    let (mut is_on_battery, battery_plug_watch) = watch_battery_plug_events(&conn, &upower).await;

    let (nag_tx, nag_rx) = tokio::sync::mpsc::channel(1);
    tokio::task::spawn_local(critical_battery_nag(nag_rx));

    loop {
        tokio::select! {
            _ = battery_plug_watch(is_on_battery) => {
                is_on_battery = !is_on_battery;
                on_ac_plug(!is_on_battery, current_battery);

                if BatteryLevel::Critical == current_battery {
                    let _res = nag_tx.send(is_on_battery).await;
                }
            },

            result = percent_changed_stream.next() => {
                let Some(message) = result else {
                    break
                };

                if let Ok(new_percent) = message.get().await {
                    match new_percent {
                        percent if percent < 10.0 => {
                            if current_battery == BatteryLevel::Critical {
                                continue
                            }

                            current_battery = BatteryLevel::Critical;
                            let _res = nag_tx.send(is_on_battery).await;

                            let now = Instant::now();
                            if now.duration_since(last_critical_notification) > Duration::from_secs(30) {
                                last_critical_notification = now;
                                let _res = Notification::new()
                                    .appname("")
                                    .summary("Battery Critical")
                                    .icon("dialog-warning-symbolic")
                                    .urgency(notify_rust::Urgency::Critical)
                                    .timeout(Duration::from_secs(30))
                                    .show_async()
                                    .await;
                            }
                        }

                        percent if percent < 20.0 => {
                            if matches!(current_battery, BatteryLevel::Low | BatteryLevel::Critical) {
                                let _res = nag_tx.send(false).await;
                                current_battery = BatteryLevel::Low;
                                continue;
                            }

                            current_battery = BatteryLevel::Low;
                            crate::pipewire::play_sound("Pop", "battery-caution");

                            let now = Instant::now();
                            if now.duration_since(last_low_notification) > Duration::from_secs(5) {
                                last_low_notification = now;
                                let _res = Notification::new()
                                    .appname("")
                                    .summary("Battery Low")
                                    .icon("dialog-warning-symbolic")
                                    .urgency(notify_rust::Urgency::Normal)
                                    .timeout(Duration::from_secs(5))
                                    .show_async()
                                    .await;
                            }
                        }

                        100.0 => {
                            if matches!(current_battery, BatteryLevel::Critical) {
                                let _res = nag_tx.send(false).await;
                            }

                            current_battery = BatteryLevel::Full;
                            crate::pipewire::play_sound("Pop", "battery-full");
                        }

                        _ => {
                            if matches!(current_battery, BatteryLevel::Critical) {
                                let _res = nag_tx.send(false).await;
                            }

                            current_battery = BatteryLevel::Normal
                        },
                    }
                }
            }
        }
    }
}

/// Repeatedly emit critical battery alert until the system begins charging.
async fn critical_battery_nag(mut watch: Receiver<bool>) {
    loop {
        match watch.recv().await {
            Some(true) => loop {
                tokio::time::sleep(Duration::from_secs(3)).await;

                match watch.try_recv() {
                    Err(TryRecvError::Empty) | Ok(true) => (),
                    _ => break,
                }

                crate::pipewire::play_sound("Pop", "battery-low");
            },
            Some(false) => (),
            None => break,
        }
    }
}

/// Play a power plug sound on an AC plug event.
fn on_ac_plug(is_plugged: bool, battery_level: BatteryLevel) {
    let (theme, sound) = if is_plugged {
        ("freedesktop", "power-plug")
    } else if Path::new("/usr/share/sounds/Pop/").exists()
        && matches!(battery_level, BatteryLevel::Low | BatteryLevel::Critical)
    {
        ("Pop", "power-unplug-battery-low")
    } else {
        ("freedesktop", "power-unplug")
    };

    crate::pipewire::play_sound(theme, sound);
}
