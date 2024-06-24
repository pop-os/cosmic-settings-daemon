use std::{error::Error, io, str::FromStr};
use tokio::fs;

use crate::LogindSessionProxy;

fn invalid_data<E: Error + Send + Sync + 'static>(err: E) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, err)
}

pub struct BrightnessDevice {
    subsystem: &'static str,
    sysname: String,
    max_absolute_brightness: u32,
}

impl BrightnessDevice {
    pub async fn new(subsystem: &'static str, sysname: String) -> io::Result<Self> {
        let path = format!("/sys/class/{}/{}/max_brightness", subsystem, &sysname);
        let value = fs::read_to_string(&path).await?;
        let max_absolute_brightness = u32::from_str(value.trim()).map_err(invalid_data)?;
        Ok(Self {
            subsystem,
            sysname,
            max_absolute_brightness,
        })
    }
    pub async fn brightness(&self) -> io::Result<u32> {
        let path = format!("/sys/class/{}/{}/brightness", self.subsystem, &self.sysname);
        let value = fs::read_to_string(&path).await?;
        let brightness = u32::from_str(value.trim()).map_err(invalid_data)?;
        Ok((brightness as f32/self.max_absolute_brightness as f32*100.0) as u32)
    }

    pub fn max_absolute_brightness(&self) -> u32 {
        self.max_absolute_brightness
    }

    pub async fn set_brightness(
        &self,
        logind_session: &LogindSessionProxy<'_>,
        value: u32,
    ) -> zbus::Result<()> {

        logind_session
            .set_brightness(self.subsystem, &self.sysname, ((value as f32)/(100 as f32) * self.max_absolute_brightness as f32) as u32)
            .await
    }

    // Matches definition in terms of percent used in gnome-settings-daemon, which seems to work
    // well enough.
    pub fn brightness_step(&self) -> u32 {
        5
    }
}
