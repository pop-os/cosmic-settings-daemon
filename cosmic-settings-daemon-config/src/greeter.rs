use cosmic_config::{cosmic_config_derive::CosmicConfigEntry, Config, CosmicConfigEntry};
use cosmic_theme::{CosmicPalette, Theme, ThemeBuilder};
use serde::{Deserialize, Serialize};
use std::{option_env, path::PathBuf};

pub const GREETER_STATE: Option<&'static str> = option_env!("GREETER_STATE");

/// State applied in the greeter during login, that should be applied in the user config
#[derive(Default, Debug, Deserialize, Serialize, Clone, CosmicConfigEntry)]
#[version = 1]
#[serde(deny_unknown_fields)]
pub struct GreeterAccessibilityState {
    pub screen_reader: Option<bool>,
    pub magnifier: Option<bool>,
    pub high_contrast: Option<bool>,
    pub invert_colors: Option<bool>,
}

impl GreeterAccessibilityState {
    pub fn path() -> PathBuf {
        PathBuf::from(GREETER_STATE.unwrap_or("/run/cosmic-greeter"))
    }

    pub fn config() -> Result<Config, cosmic_config::Error> {
        let helper = Config::with_custom_path(
            crate::NAME,
            GreeterAccessibilityState::VERSION,
            GreeterAccessibilityState::path(),
        )?;

        Ok(helper)
    }
}

pub fn apply_hc_theme(enabled: bool) -> Result<Config, cosmic_config::Error> {
    let set_hc = |is_dark: bool| {
        let builder_config = if is_dark {
            ThemeBuilder::dark_config()?
        } else {
            ThemeBuilder::light_config()?
        };
        let mut builder = match ThemeBuilder::get_entry(&builder_config) {
            Ok(b) => b,
            Err((errs, b)) => {
                eprintln!("{errs:?}");
                b
            }
        };

        builder.palette = if is_dark {
            if enabled {
                CosmicPalette::HighContrastDark(builder.palette.inner())
            } else {
                CosmicPalette::Dark(builder.palette.inner())
            }
        } else if enabled {
            CosmicPalette::HighContrastLight(builder.palette.inner())
        } else {
            CosmicPalette::Light(builder.palette.inner())
        };
        builder.write_entry(&builder_config)?;

        let new_theme = builder.build();

        let theme_config = if is_dark {
            Theme::dark_config()?
        } else {
            Theme::light_config()?
        };

        new_theme.write_entry(&theme_config)?;

        Result::<(), cosmic_config::Error>::Ok(())
    };
    let res = set_hc(true);
    set_hc(false)?;
    res
}
