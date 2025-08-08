use std::time::Duration;

use cosmic_config::CosmicConfigEntry;
use cosmic_dbus_a11y::*;
use cosmic_settings_daemon_config::greeter;

pub fn sync_with_greeter() -> anyhow::Result<()> {
    log::trace!("syncing with greeter...");
    let helper = greeter::GreeterAccessibilityState::config()?;
    let state = match greeter::GreeterAccessibilityState::get_entry(&helper) {
        Ok(s) => s,
        Err((errs, s)) => {
            for err in errs {
                log::error!("Error loading greeter state: {err:?}");
            }
            s
        }
    };

    if let Some(hc) = state.high_contrast {
        if let Err(err) = greeter::apply_hc_theme(hc) {
            log::error!("Failed to apply high contrast changes from the greeter: {err:?}");
        }
    }

    if let Some(screen_reader) = state.screen_reader {
        tokio::spawn(async move {
            for _ in 0..5 {
                let conn = match zbus::Connection::session().await.map_err(|e| e.to_string()) {
                    Ok(conn) => conn,
                    Err(err) => {
                        log::error!("Failed to connect to session message bus: {err:?}");
                        _ = tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                };
                if let Ok(proxy) = StatusProxy::new(&conn).await {
                    if let Err(err) = proxy.set_screen_reader_enabled(screen_reader).await {
                        log::error!("Failed to apply screen reader status. {err:?}");
                        continue;
                    }
                }
                break;
            }
        });
    }

    log::trace!("applied greeter state");

    Ok(())
}
