use cosmic_config::{Config as CosmicConfig, ConfigGet};

const AUDIO_CONFIG: &str = "com.system76.CosmicAudio";
const AMPLIFICATION_SINK: &str = "amplification_sink";

pub async fn amplification_sink() -> bool {
    match CosmicConfig::new(AUDIO_CONFIG, 1) {
        Ok(config) => config.get::<bool>(AMPLIFICATION_SINK).unwrap_or(true),
        Err(e) => {
            tracing::debug!("Failed to read audio amplification config: {}", e);
            true
        }
    }
}
