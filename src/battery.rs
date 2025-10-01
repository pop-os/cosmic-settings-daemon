use acpid_plug::AcPlugEvents;
use notify_rust::Notification;
use std::time::Instant;
use std::{path::Path, time::Duration};
use tokio::sync::mpsc::Sender;
use tokio::sync::mpsc::{Receiver, error::TryRecvError};
use tokio_stream::StreamExt;
use upower_dbus::BatteryLevel;
use zbus::Connection;

// TODO: Add config parameter for changing the preferred sound theme.

pub async fn monitor() {
    let Ok(ac_plug_events) = acpid_plug::connect().await else {
        return;
    };

    let ac_plugged = ac_plug_events.plugged();
    let (ac_plug_tx, ac_plug_rx) = tokio::sync::mpsc::channel(1);
    tokio::task::spawn_local(ac_plug_monitor(ac_plug_events, ac_plug_tx));
    low_power_monitor(ac_plugged, ac_plug_rx).await;
}

/// Watch AC plug events and emit sounds on plug event changes.
pub async fn ac_plug_monitor(
    mut ac_plug_events: AcPlugEvents,
    ac_plug_tx: Sender<acpid_plug::Event>,
) {
    if let Some(Ok(event)) = ac_plug_events.next().await {
        let _res = ac_plug_tx.send(event).await;

        // Use a tokio watch channel to debounce the ac plug events.
        let (tx, mut rx) = tokio::sync::watch::channel(event);

        tokio::task::spawn_local(async move {
            while let Some(Ok(event)) = ac_plug_events.next().await {
                if tx.send(event).is_err() {
                    break;
                }
            }
        });

        // Listen for changes to the AC plug state.
        while let Ok(()) = rx.changed().await {
            let _res = ac_plug_tx.send(*rx.borrow_and_update()).await;

            // Wait at least 500 ms before checking for another change.
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}

pub async fn low_power_monitor(mut ac_plugged: bool, mut ac_plug_rx: Receiver<acpid_plug::Event>) {
    let Ok(conn) = Connection::system().await else {
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

    let (nag_tx, nag_rx) = tokio::sync::mpsc::channel(1);

    tokio::task::spawn_local(critical_battery_nag(nag_rx));

    loop {
        tokio::select! {
            event = ac_plug_rx.recv() => {
                let Some(event) = event else {
                    break
                };

                ac_plugged = event == acpid_plug::Event::Plugged;

                on_ac_plug(event, current_battery);

                if BatteryLevel::Critical == current_battery {
                    let _res = nag_tx.send(!ac_plugged).await;
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
                            let _res = nag_tx.send(!ac_plugged).await;

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
fn on_ac_plug(event: acpid_plug::Event, battery_level: BatteryLevel) {
    let (theme, sound) = if matches!(event, acpid_plug::Event::Plugged) {
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
