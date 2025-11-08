use std::{error::Error, io, str::FromStr, time::Duration};
use tokio::fs;
use tokio::time::{self, Instant};

use crate::LogindSessionProxy;

fn invalid_data<E: Error + Send + Sync + 'static>(err: E) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, err)
}

pub struct BrightnessDevice {
    subsystem: &'static str,
    sysname: String,
    max_brightness: u32,
}

impl BrightnessDevice {
    pub async fn new(subsystem: &'static str, sysname: String) -> io::Result<Self> {
        let path = format!("/sys/class/{}/{}/max_brightness", subsystem, &sysname);
        let value = fs::read_to_string(&path).await?;
        let max_brightness = u32::from_str(value.trim()).map_err(invalid_data)?;
        Ok(Self {
            subsystem,
            sysname,
            max_brightness,
        })
    }
    pub async fn brightness(&self) -> io::Result<u32> {
        let path = format!("/sys/class/{}/{}/brightness", self.subsystem, &self.sysname);
        let value = fs::read_to_string(&path).await?;
        Ok(u32::from_str(value.trim()).map_err(invalid_data)?)
    }

    async fn actual_brightness(&self) -> io::Result<Option<u32>> {
        let path = format!("/sys/class/{}/{}/actual_brightness", self.subsystem, &self.sysname);
        match fs::read_to_string(&path).await {
            Ok(s) => Ok(Some(u32::from_str(s.trim()).map_err(invalid_data)?)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
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
        if self.subsystem != "backlight" || target == 0 {
            return Ok(());
        }
        let max = self.max_brightness();
        // If the driver doesn't expose actual_brightness, do nothing.
        let Ok(Some(initial_ab)) = self.actual_brightness().await else {
            return Ok(());
        };
        if initial_ab > 0 {
            return Ok(());
        }
        const SETTLE_WINDOW: Duration = Duration::from_millis(40);
        const POLL_INTERVAL: Duration = Duration::from_millis(3);
        const MAX_BUMPS: u32 = 3;
        let mut current_target = target;
        // Bound the number of guarded increments to avoid ratcheting to max on buggy drivers.
        for _ in 0..MAX_BUMPS {
            let start = Instant::now();
            loop {
                match self.actual_brightness().await {
                    Ok(Some(value)) if value > 0 => return Ok(()),
                    Ok(Some(_)) => {}
                    Ok(None) => return Ok(()),
                    Err(err) => {
                        log::debug!(
                            "Stopping brightness guard due to actual_brightness read failure: {err}"
                        );
                        return Ok(());
                    }
                }
                let elapsed = start.elapsed();
                if elapsed >= SETTLE_WINDOW {
                    log::debug!(
                        "Brightness guard waited {:?} but actual_brightness stayed at 0 (target {}).",
                        SETTLE_WINDOW,
                        current_target
                    );
                    break;
                }
                let remaining = SETTLE_WINDOW - elapsed;
                let sleep_for = if remaining > POLL_INTERVAL {
                    POLL_INTERVAL
                } else {
                    remaining
                };
                if sleep_for == Duration::ZERO {
                    break;
                }
                time::sleep(sleep_for).await;
            }
            if current_target >= max {
                log::debug!(
                    "Brightness guard reached max brightness ({}) while actual_brightness stayed at 0.",
                    current_target
                );
                break;
            }
            let next_target = current_target.saturating_add(1).min(max);
            if next_target == current_target {
                break;
            }
            current_target = next_target;
            logind_session
                .set_brightness(self.subsystem, &self.sysname, current_target)
                .await?;
            if current_target == max {
                log::debug!(
                    "Brightness guard bumped brightness to max ({}) after waiting for visibility.",
                    current_target
                );
            }
        }
        Ok(())
    }

    pub fn max_brightness(&self) -> u32 {
        self.max_brightness
    }

    pub fn min_brightness(&self) -> u32 {
        if self.subsystem == "backlight" {
            if self.max_brightness <= 20 { 0 } else { 1 }
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
        let clamped = value.clamp(self.min_brightness(), self.max_brightness);
        logind_session
            .set_brightness(self.subsystem, &self.sysname, clamped)
            .await?;
        // If panel still effectively off (e.g., OLED 0..3), bump minimally until visible.
        self.ensure_visible_after_write(logind_session, clamped).await
    }
}
