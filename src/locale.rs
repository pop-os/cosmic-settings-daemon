use std::time::Duration;

use anyhow::Context;
use cosmic_comp_config::XkbConfig;
use cosmic_config::{ConfigGet, ConfigSet};
use tokio::sync::mpsc::Receiver;
use tokio_stream::StreamExt;

pub const COSMIC_COMP_ID: &'static str = "com.system76.CosmicComp";
pub const COSMIC_COMP_XDG_KEY: &'static str = "xkb_config";

pub async fn sync_locale1(mut rx: Receiver<()>) -> anyhow::Result<()> {
    let conn = zbus::Connection::system().await?;
    let proxy = locale1::locale1Proxy::new(&conn).await?;
    let config = cosmic_config::Config::new(COSMIC_COMP_ID, 1)
        .context("Found no cosmic-comp configuration")?;

    let mut model_stream = proxy.receive_x11model_changed().await;
    let mut layout_stream = proxy.receive_x11layout_changed().await;
    let mut variant_stream = proxy.receive_x11variant_changed().await;
    let mut options_stream = proxy.receive_x11options_changed().await;

    // Consume events that are emitted on startup.
    _ = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            tokio::select! {
                _ = model_stream.next() => (),
                _ = layout_stream.next() => (),
                _ = variant_stream.next() => (),
                _ = options_stream.next() => (),
            }
        }
    })
    .await;

    // Sync on startup only if the xkb config is not set.
    if config.get::<XkbConfig>(COSMIC_COMP_XDG_KEY).is_err() {
        _ = sync_locale1_to_cosmic(&config, &proxy).await;
    } else {
        _ = sync_cosmic_to_locale1(&config, &proxy).await;
    }

    loop {
        if let Err(err) = tokio::select! {
            received = rx.recv() => {
                if received.is_some() {
                    sync_cosmic_to_locale1(&config, &proxy).await
                } else {
                    Ok(())
                }
            },
            _ = model_stream.next() => sync_locale1_to_cosmic(&config, &proxy).await,
            _ = layout_stream.next() => sync_locale1_to_cosmic(&config, &proxy).await,
            _ = variant_stream.next() => sync_locale1_to_cosmic(&config, &proxy).await,
            _ = options_stream.next() => sync_locale1_to_cosmic(&config, &proxy).await,
        } {
            eprintln!("Failed to sync xkb_config with systemd-localed: {}", err);
        };
    }
}

async fn sync_cosmic_to_locale1(
    config: &cosmic_config::Config,
    proxy: &locale1::locale1Proxy<'_>,
) -> anyhow::Result<()> {
    let xkb_config = config
        .get::<XkbConfig>(COSMIC_COMP_XDG_KEY)
        .context("xkb-config not set")?;

    proxy
        .set_x11keyboard(
            &xkb_config.layout,
            &xkb_config.model,
            &xkb_config.variant,
            xkb_config.options.as_deref().unwrap_or(""),
            true,
            false,
        )
        .await
        .context("Failed to update systemd-locale1 from xkb_config")?;
    Ok(())
}

async fn sync_locale1_to_cosmic(
    config: &cosmic_config::Config,
    proxy: &locale1::locale1Proxy<'_>,
) -> anyhow::Result<()> {
    let (model, layout, variant, options) = futures_util::try_join!(
        proxy.x11model(),
        proxy.x11layout(),
        proxy.x11variant(),
        proxy.x11options()
    )
    .context("failed to get xkb config from locale1 daemon")?;

    let current_config = config
        .get::<XkbConfig>(COSMIC_COMP_XDG_KEY)
        .unwrap_or_default();

    let new_config = XkbConfig {
        model,
        layout,
        variant,
        options: match options.as_str() {
            "" => None,
            x => Some(x.to_string()),
        },
        ..current_config.clone()
    };

    if new_config == current_config {
        return Ok(());
    }

    config
        .set::<XkbConfig>(COSMIC_COMP_XDG_KEY, new_config)
        .context("Failed to update xkb_config from systemd-localed")?;
    Ok(())
}
