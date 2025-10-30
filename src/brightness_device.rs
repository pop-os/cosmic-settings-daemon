use std::{error::Error, io, str::FromStr};
use tokio::fs;

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

    pub fn max_brightness(&self) -> u32 {
        self.max_brightness
    }

    pub async fn set_brightness(
        &self,
        logind_session: &LogindSessionProxy<'_>,
        value: u32,
    ) -> zbus::Result<()> {
        // Never set 0 on LCD panel backlights; it blanks the screen.
        // Keyboard LEDs and other subsystems can still use 0.
        let min = if self.subsystem == "backlight" { 1 } else { 0 };
        let clamped = value.clamp(min, self.max_brightness);
        logind_session
            .set_brightness(self.subsystem, &self.sysname, clamped)
            .await
    }
}
