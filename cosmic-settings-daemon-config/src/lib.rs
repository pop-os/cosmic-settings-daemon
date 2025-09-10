use cosmic_config::{Config, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};

#[cfg(feature = "greeter")]
pub mod greeter;

pub const NAME: &str = "com.system76.CosmicSettingsDaemon";

/// Config structure for settings managed by the daemon
#[derive(Default, Debug, Deserialize, Serialize, Clone, CosmicConfigEntry)]
#[version = 1]
#[serde(deny_unknown_fields)]
pub struct CosmicSettingsDaemonConfig {
    pub mono_sound: bool,
}

/// Config structure for settings managed by the daemon
#[derive(Default, Debug, Deserialize, Serialize, Clone, CosmicConfigEntry)]
#[version = 1]
#[serde(deny_unknown_fields)]
pub struct CosmicSettingsDaemonState {
    /// the sink that the virtual mono sink is attached to
    pub default_sink_name: String,
}

impl CosmicSettingsDaemonConfig {
    pub fn config() -> Result<Config, cosmic_config::Error> {
        Config::new(NAME, Self::VERSION)
    }
}

impl CosmicSettingsDaemonState {
    pub fn config() -> Result<Config, cosmic_config::Error> {
        Config::new_state(NAME, Self::VERSION)
    }
}
