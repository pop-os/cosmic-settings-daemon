use cosmic_config::{Config as CosmicConfig, ConfigGet};

const AUDIO_CONFIG: &str = "com.system76.CosmicAudio";
const AMPLIFICATION_SINK: &str = "amplification_sink";
const VOLUME_STEP: &str = "volume_step";

pub async fn amplification_sink() -> bool {
    match CosmicConfig::new(AUDIO_CONFIG, 1) {
        Ok(config) => config.get::<bool>(AMPLIFICATION_SINK).unwrap_or(true),
        Err(e) => {
            tracing::debug!("Failed to read audio amplification config: {}", e);
            true
        }
    }
}

pub async fn volume_step() -> u32 {
    match CosmicConfig::new(AUDIO_CONFIG, 1) {
        Ok(config) => config
            .get::<u32>(VOLUME_STEP)
            .map(|v| v.max(1))
            .unwrap_or(5),
        Err(e) => {
            tracing::debug!("Failed to read volume step config: {}", e);
            5
        }
    }
}
