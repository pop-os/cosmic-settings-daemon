use tokio::sync::broadcast;

pub mod backend;
pub mod sensor_proxy;
pub mod sysfs;

pub use backend::AmbientLightBackend;

pub async fn monitor(mut shutdown_rx: broadcast::Receiver<()>) {
    log::info!("ambient-light: starting ambient light sensor probe");

    let Some(backend) = AmbientLightBackend::detect().await else {
        log::warn!("ambient-light: no ambient light sensor backend detected");
        return;
    };

    log::info!("ambient-light: using {} backend", backend.name());

    loop {
        tokio::select! {
            _ = shutdown_rx.recv() => {
                log::info!("ambient-light: stopping ambient light sensor probe");
                backend.release().await;
                break;
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {
                if let Some(lux) = backend.read_lux().await {
                    log::info!(
                        "ambient-light: backend={} lux={:.3}",
                        backend.name(),
                        lux
                    );
                }
            }
        }
    }
}
