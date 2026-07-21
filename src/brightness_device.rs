use ddc_hi::{Ddc, Display};
use std::error::Error;
use std::io;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::{fs, time};

use crate::LogindSessionProxy;

const BRIGHTNESS: u8 = 0x10;

fn invalid_data<E: Error + Send + Sync + 'static>(err: E) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, err)
}

pub struct BrightnessDevice {
    subsystem: Option<&'static str>,
    sysname: Option<String>,
    max_brightness: Option<u32>,
    brightness_dcc: Arc<(Mutex<Option<u16>>, std::sync::Condvar)>,
}

impl Drop for BrightnessDevice {
    fn drop(&mut self) {
        let mut g = self.brightness_dcc.0.lock().unwrap();
        *g = None;

        self.brightness_dcc.1.notify_all();
    }
}

impl BrightnessDevice {
    pub fn external() -> Self {
        let v: Arc<(Mutex<Option<u16>>, std::sync::Condvar)> =
            Arc::new((Mutex::new(Some(100u16)), std::sync::Condvar::new()));
        let brightness_dcc = v.clone();
        let displays_empty = Display::enumerate().is_empty();

        std::thread::spawn(move || {
            // How long cached DDC display handles are kept after the last
            // brightness change. Each handle is an open fd on a /dev/i2c-*
            // adapter; holding them indefinitely blocks the kernel's DP-MST
            // teardown on monitor unplug, permanently wedging drm_dp_mst_wq
            // (https://github.com/pop-os/cosmic-settings-daemon/issues/165).
            // The cache still avoids re-enumeration during a burst of
            // brightness key repeats.
            const IDLE_TIMEOUT: Duration = Duration::from_secs(2);

            let mut displays: Vec<Display> = Vec::new();
            let mut cur = 100;
            'outer: loop {
                let brightness = {
                    let mut guard = v.0.lock().unwrap();
                    loop {
                        match *guard {
                            None => break 'outer,
                            Some(b) if b != cur => break b,
                            Some(_) => {}
                        }
                        if displays.is_empty() {
                            guard = v.1.wait(guard).unwrap();
                        } else {
                            let (g, res) = v.1.wait_timeout(guard, IDLE_TIMEOUT).unwrap();
                            guard = g;
                            if res.timed_out() {
                                // Idle: release the i2c fds so unplugged
                                // displays can be torn down by the kernel.
                                displays.clear();
                            }
                        }
                    }
                };

                cur = brightness;
                if displays.is_empty() {
                    displays = Display::enumerate();
                }
                for display in &mut displays {
                    if display.update_capabilities().is_err() {
                        continue;
                    }

                    if let Err(err) = display.handle.set_vcp_feature(BRIGHTNESS, brightness) {
                        log::error!("Failed to set brightness: {err:?}");
                    }
                }
            }
        });

        Self {
            subsystem: None,
            sysname: None,
            max_brightness: if displays_empty { None } else { Some(100) },
            brightness_dcc,
        }
    }

    pub async fn new(subsystem: &'static str, sysname: String) -> io::Result<Self> {
        let path = format!("/sys/class/{}/{}/max_brightness", subsystem, sysname);
        let value = fs::read_to_string(&path).await?;
        let max_brightness = u32::from_str(value.trim()).map_err(invalid_data)?;
        let mut external = Self::external();
        external.subsystem = Some(subsystem);
        external.sysname = Some(sysname);
        external.max_brightness = Some(max_brightness);
        Ok(external)
    }

    pub async fn brightness(&self) -> io::Result<u32> {
        let mut ret = io::Result::Err(io::Error::other("No display"));
        if let Some((subsystem, sysname)) = self.subsystem.as_ref().zip(self.sysname.as_ref()) {
            let path = format!("/sys/class/{}/{}/brightness", subsystem, sysname);
            let value = fs::read_to_string(&path).await?;
            ret = u32::from_str(value.trim()).map_err(invalid_data);
        }

        if ret.is_err() {
            for mut d in Display::enumerate() {
                if d.update_capabilities().is_err() {
                    continue;
                }
                if let Some(feature) = d.info.mccs_database.get(BRIGHTNESS)
                    && let Ok(value) = d.handle.get_vcp_feature(feature.code)
                {
                    return Ok(value.value() as u32);
                }
            }
        }
        ret
    }

    async fn actual_brightness(&self) -> io::Result<Option<u32>> {
        if let Some((subsystem, sysname)) = self.subsystem.zip(self.sysname.as_ref()) {
            let path = format!("/sys/class/{}/{}/actual_brightness", subsystem, sysname);
            match fs::read_to_string(&path).await {
                Ok(s) => Ok(Some(u32::from_str(s.trim()).map_err(invalid_data)?)),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e),
            }
        } else {
            io::Result::Err(io::Error::other("No display"))
        }
    }

    /// After an initial non-zero write, if `actual_brightness == 0`, increment by +1 until visible
    /// or until we reach max. Swallows `actual_brightness` read errors (no guard in that case).
    async fn ensure_visible_after_write(
        &self,
        logind_session: &LogindSessionProxy<'_>,
        target: u32,
    ) -> zbus::Result<()> {
        // Only makes sense for display backlights with a non-zero intent.
        if self.subsystem.is_none_or(|s| s != "backlight") || target == 0 {
            return Ok(());
        }
        let max = self.max_brightness();
        if max <= 0 {
            return Ok(());
        }
        // Only guard near the bottom of the range (≈5%). Higher levels are assumed visible.
        if target > max as u32 / 20 {
            return Ok(());
        }
        const SETTLE_WINDOW: Duration = Duration::from_millis(40);
        const MAX_BUMPS: u32 = 3;
        let mut current_target = target;
        for _ in 0..MAX_BUMPS {
            // Give the driver time to apply the new level before we judge visibility.
            time::sleep(SETTLE_WINDOW).await;
            match self.actual_brightness().await {
                // Still effectively off after settling: try a small bump.
                Ok(Some(0)) => {
                    // fall through to bump logic below
                }
                // Visible (non-zero) or no separate actual_brightness: stop guarding.
                Ok(Some(_)) | Ok(None) => return Ok(()),
                // Read error: stop guarding rather than looping.
                Err(err) => {
                    log::debug!(
                        "Stopping brightness guard due to actual_brightness read failure: {err}"
                    );
                    return Ok(());
                }
            }
            if current_target >= max as u32 {
                log::debug!(
                    "Brightness guard reached max brightness ({}) while actual_brightness stayed at 0.",
                    current_target
                );
                break;
            }
            let next_target = current_target.saturating_add(1).min(max as u32);
            if next_target == current_target {
                break;
            }
            current_target = next_target;
            if let Some((subsystem, sysname)) = self.subsystem.zip(self.sysname.as_ref()) {
                logind_session
                    .set_brightness(subsystem, sysname, current_target)
                    .await?;
            }
            log::debug!(
                "Brightness guard bumped backlight brightness to {} after detecting ab==0.",
                current_target
            );
        }
        Ok(())
    }

    pub fn max_brightness(&self) -> i32 {
        self.max_brightness.map_or_else(|| -1, |v| v as i32)
    }

    pub fn min_brightness(&self) -> u32 {
        if self.subsystem.is_some_and(|d| d == "backlight") {
            if self.max_brightness.is_some_and(|b| b <= 20) {
                0
            } else {
                1
            }
        } else {
            0
        }
    }

    pub async fn set_brightness(
        &self,
        logind_session: &LogindSessionProxy<'_>,
        value: u32,
    ) -> zbus::Result<()> {
        // Never set 0 on LCD backlights unless the device is clearly coarse (<=20 levels).
        // Keyboard LEDs and other subsystems can still use 0.
        let clamped = value.clamp(self.min_brightness(), self.max_brightness.unwrap_or(100));

        let b_dcc = (clamped * 100 / self.max_brightness.unwrap_or(100)) as u16;
        {
            let mut g = self.brightness_dcc.0.lock().unwrap();
            *g = Some(b_dcc);
        }

        self.brightness_dcc.1.notify_all();

        if let Some((subsystem, sysname)) = self.subsystem.zip(self.sysname.as_ref()) {
            logind_session
                .set_brightness(subsystem, sysname, clamped)
                .await?;
        }
        // If panel still effectively off (e.g., OLED 0..3), bump minimally until visible.
        self.ensure_visible_after_write(logind_session, clamped)
            .await?;

        Ok(())
    }
}
