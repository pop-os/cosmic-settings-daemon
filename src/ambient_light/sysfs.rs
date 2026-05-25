use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SysfsAls {
    device_path: PathBuf,
}

impl SysfsAls {
    pub fn new() -> Option<Self> {
        Self::new_from(Path::new("/sys/bus/iio/devices"))
    }

    fn new_from(iio_dir: &Path) -> Option<Self> {
        let entries = fs::read_dir(iio_dir).ok()?;

        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };

            if !file_name.starts_with("iio:device") {
                continue;
            }

            let Ok(name) = fs::read_to_string(path.join("name")) else {
                continue;
            };

            if is_als_name(name.trim()) && path.join("in_illuminance_raw").exists() {
                log::debug!("ambient-light: found sysfs device at {:?}", path);
                return Some(Self { device_path: path });
            }
        }

        None
    }

    pub fn read_lux(&self) -> Option<f64> {
        let raw = read_f64(&self.device_path.join("in_illuminance_raw"))?;
        let scale_path = self.device_path.join("in_illuminance_scale");
        let scale = if scale_path.exists() {
            read_f64(&scale_path)?
        } else {
            1.0
        };

        Some(raw * scale)
    }

    pub fn path(&self) -> &Path {
        &self.device_path
    }
}

fn is_als_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name == "als" || name.contains("ambient") || name.contains("light")
}

fn read_f64(path: &Path) -> Option<f64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "cosmic-settings-daemon-als-test-{}-{nanos}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self { path }
        }

        fn device(&self, name: &str, raw: Option<&str>, scale: Option<&str>) -> PathBuf {
            let path = self.path.join("iio:device0");
            fs::create_dir(&path).unwrap();
            fs::write(path.join("name"), name).unwrap();
            if let Some(raw) = raw {
                fs::write(path.join("in_illuminance_raw"), raw).unwrap();
            }
            if let Some(scale) = scale {
                fs::write(path.join("in_illuminance_scale"), scale).unwrap();
            }
            path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn detects_case_insensitive_als_name() {
        let dir = TestDir::new();
        let device = dir.device("Ambient Light", Some("42"), Some("0.5"));

        let als = SysfsAls::new_from(&dir.path).unwrap();

        assert_eq!(als.path(), device.as_path());
        assert_eq!(als.read_lux(), Some(21.0));
    }

    #[test]
    fn ignores_devices_without_illuminance_raw() {
        let dir = TestDir::new();
        dir.device("als", None, Some("0.5"));

        assert!(SysfsAls::new_from(&dir.path).is_none());
    }

    #[test]
    fn uses_scale_one_when_scale_file_is_absent() {
        let dir = TestDir::new();
        dir.device("als", Some("42"), None);

        let als = SysfsAls::new_from(&dir.path).unwrap();

        assert_eq!(als.read_lux(), Some(42.0));
    }

    #[test]
    fn returns_none_when_scale_is_invalid() {
        let dir = TestDir::new();
        dir.device("als", Some("42"), Some("nope"));

        let als = SysfsAls::new_from(&dir.path).unwrap();

        assert_eq!(als.read_lux(), None);
    }
}
