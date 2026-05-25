use crate::ambient_light::sensor_proxy::SensorProxyProxy;
use crate::ambient_light::sysfs::SysfsAls;

#[derive(Debug)]
pub enum AmbientLightBackend {
    SensorProxy(SensorProxyProxy<'static>),
    Sysfs(SysfsAls),
}

impl AmbientLightBackend {
    pub async fn detect() -> Option<Self> {
        if let Some(proxy) = Self::detect_sensor_proxy().await {
            return Some(AmbientLightBackend::SensorProxy(proxy));
        }

        SysfsAls::new().map(|sysfs_als| {
            log::info!(
                "ambient-light: using sysfs fallback at {:?}",
                sysfs_als.path()
            );
            AmbientLightBackend::Sysfs(sysfs_als)
        })
    }

    async fn detect_sensor_proxy() -> Option<SensorProxyProxy<'static>> {
        log::debug!("ambient-light: trying iio-sensor-proxy over system D-Bus");

        let conn = match zbus::Connection::system().await {
            Ok(conn) => conn,
            Err(err) => {
                log::debug!("ambient-light: system D-Bus is not available: {err:?}");
                return None;
            }
        };

        let proxy = match SensorProxyProxy::builder(&conn).build().await {
            Ok(proxy) => proxy,
            Err(err) => {
                log::debug!("ambient-light: failed to build iio-sensor-proxy client: {err:?}");
                return None;
            }
        };

        match proxy.has_ambient_light().await {
            Ok(true) => {}
            Ok(false) => {
                log::debug!("ambient-light: iio-sensor-proxy reports no ambient light sensor");
                return None;
            }
            Err(err) => {
                log::warn!("ambient-light: failed to read HasAmbientLight: {err:?}");
                return None;
            }
        }

        match proxy.light_level_unit().await {
            Ok(unit) if unit == "lux" => {}
            Ok(unit) => {
                log::warn!(
                    "ambient-light: iio-sensor-proxy light unit is {unit:?}, expected \"lux\""
                );
                return None;
            }
            Err(err) => {
                log::warn!("ambient-light: failed to read LightLevelUnit: {err:?}");
                return None;
            }
        }

        match proxy.claim_light().await {
            Ok(()) => Some(proxy),
            Err(err) => {
                log::warn!("ambient-light: failed to claim iio-sensor-proxy light sensor: {err:?}");
                None
            }
        }
    }

    pub async fn read_lux(&self) -> Option<f64> {
        match self {
            AmbientLightBackend::SensorProxy(proxy) => match proxy.light_level().await {
                Ok(lux) => Some(lux),
                Err(err) => {
                    log::warn!("ambient-light: failed to read LightLevel: {err:?}");
                    None
                }
            },
            AmbientLightBackend::Sysfs(sysfs_als) => sysfs_als.read_lux(),
        }
    }

    pub async fn release(&self) {
        match self {
            AmbientLightBackend::SensorProxy(proxy) => {
                if let Err(err) = proxy.release_light().await {
                    log::debug!("ambient-light: failed to release iio-sensor-proxy claim: {err:?}");
                }
            }
            AmbientLightBackend::Sysfs(_) => {}
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            AmbientLightBackend::SensorProxy(_) => "iio-sensor-proxy",
            AmbientLightBackend::Sysfs(_) => "sysfs",
        }
    }
}
