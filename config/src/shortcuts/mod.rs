// SPDX-License-Identifier: MPL-2.0

pub mod action;
pub use action::Action;

pub mod modifier;

pub use modifier::{Modifier, Modifiers, ModifiersDef};

mod binding;
pub use binding::Binding;

pub mod sym;

use cosmic_config::cosmic_config_derive::CosmicConfigEntry;
use cosmic_config::{ConfigGet, CosmicConfigEntry};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::HashMap;
use xkbcommon::xkb;

pub const ID: &str = "com.system76.CosmicSettings.Shortcuts";

pub type SystemActions = BTreeMap<action::System, String>;

/// Gets a cosmic-config [Config] context.
pub fn context() -> Result<cosmic_config::Config, cosmic_config::Error> {
    Config::context()
}

/// Get the current system shortcut configuration
///
/// Merges user-defined custom shortcuts to the system default config
pub fn shortcuts(context: &cosmic_config::Config) -> Shortcuts {
    // Load shortcuts defined by the system.
    let mut shortcuts = context.get::<Shortcuts>("defaults").unwrap_or_else(|why| {
        tracing::error!("shortcuts defaults config error: {why:?}");
        Shortcuts::default()
    });

    // Load custom shortcuts defined by the user.
    let custom_shortcuts = context.get::<Shortcuts>("custom").unwrap_or_else(|why| {
        tracing::error!("shortcuts custom config error: {why:?}");
        Shortcuts::default()
    });

    // Combine while overriding system shortcuts.
    shortcuts.0.extend(custom_shortcuts.0);
    shortcuts
}

/// Get a map of system actions and their configured commands
pub fn system_actions(context: &cosmic_config::Config) -> SystemActions {
    let mut config = SystemActions::default();

    // Get the system config first
    if let Ok(context) = cosmic_config::Config::system(ID, Config::VERSION) {
        match context.get::<SystemActionsImpl>("system_actions") {
            Ok(system_config) => config = system_config.0,
            Err(why) => {
                tracing::error!("failed to read system shortcuts config 'system_actions': {why:?}");
            }
        }
    }

    // Then override it with the user's config
    match context.get::<SystemActionsImpl>("system_actions") {
        Ok(user_config) => config.extend(user_config.0),
        Err(why) => {
            tracing::error!("failed to read local shortcuts config 'system_actions': {why:?}");
        }
    }

    config
}

/// cosmic-config configuration state for `com.system76.CosmicSettings.Shortcuts`
#[derive(Clone, Debug, Default, PartialEq, CosmicConfigEntry)]
#[version = 1]
pub struct Config {
    pub defaults: Shortcuts,
    pub custom: Shortcuts,
    pub system_actions: SystemActions,
}

impl Config {
    pub fn context() -> Result<cosmic_config::Config, cosmic_config::Error> {
        cosmic_config::Config::new(ID, Self::VERSION)
    }

    pub fn shortcuts(&self) -> impl Iterator<Item = (&Binding, &Action)> {
        self.custom.iter().chain(self.defaults.iter())
    }

    pub fn shortcut_for_action(&self, action: &Action) -> Option<String> {
        self.custom
            .shortcut_for_action(action)
            .or_else(|| self.defaults.shortcut_for_action(action))
    }
}

/// A map of defined key [Binding]s and their triggerable [Action]s
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Shortcuts(pub HashMap<Binding, Action>);

struct ShortcutMapVisitor;

impl<'de> serde::de::Visitor<'de> for ShortcutMapVisitor {
    type Value = Shortcuts;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("Shortcuts Map")
    }

    fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
    where
        M: serde::de::MapAccess<'de>,
    {
        let mut map = HashMap::with_capacity(access.size_hint().unwrap_or(0));

        while let Some((binding, action)) = access.next_entry::<Binding, &ron::value::RawValue>()? {
            match action.into_rust::<Action>() {
                Ok(val) => {
                    map.insert(binding, val);
                }
                Err(err) => {
                    tracing::warn!(
                        "Skipping over invalid Action ({}): {}",
                        action.get_ron(),
                        err
                    );
                    map.insert(binding, Action::Disable);
                }
            };
        }

        Ok(Shortcuts(map))
    }
}

impl<'de> Deserialize<'de> for Shortcuts {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        deserializer.deserialize_map(ShortcutMapVisitor)
    }
}

impl Shortcuts {
    // pub fn default_shortcuts() -> Self {
    //     Shortcuts(HashMap::from([
    //         (Binding::new(Modifiers::new()))
    //     ]))
    // }

    pub fn insert_default_binding(
        &mut self,
        modifiers: Modifiers,
        keys: impl Iterator<Item = xkb::Keysym>,
        action: Action,
    ) {
        if !self.0.values().any(|a| a == &action) {
            for key in keys {
                let pattern = Binding {
                    description: None,
                    modifiers: modifiers.clone(),
                    key: Some(key),
                };
                if !self.0.contains_key(&pattern) {
                    self.0.insert(pattern, action.clone());
                }
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Binding, &Action)> {
        self.0.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&Binding, &mut Action)> {
        self.0.iter_mut()
    }

    pub fn shortcut_for_action(&self, action: &Action) -> Option<String> {
        self.shortcuts(action)
            .find(|b| b.key.is_none()) // prefer short bindings
            .or_else(|| {
                self.shortcuts(action).find(|b| {
                    // prefer bindings containing arrow keys
                    matches!(
                        b.key,
                        Some(xkb::Keysym::Down)
                            | Some(xkb::Keysym::Up)
                            | Some(xkb::Keysym::Left)
                            | Some(xkb::Keysym::Right)
                    )
                })
            })
            .or_else(|| self.shortcuts(action).next()) // take the first one
            .map(|binding| binding.to_string())
    }

    pub fn shortcuts<'a>(&'a self, action: &'a Action) -> impl Iterator<Item = &'a Binding> {
        self.0
            .iter()
            .filter(move |(_, a)| *a == action)
            .map(|(b, _)| b)
    }
}

/// Whether a key is pressed or released.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum State {
    Pressed,
    Released,
}

pub struct SystemActionsImpl(SystemActions);

struct SystemActionsMapVisitor;

impl<'de> serde::de::Visitor<'de> for SystemActionsMapVisitor {
    type Value = SystemActionsImpl;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("SystemActions Map")
    }

    fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
    where
        M: serde::de::MapAccess<'de>,
    {
        let mut map = BTreeMap::new();

        while let Some((action, command)) = access.next_entry::<&ron::value::RawValue, String>()? {
            match action.into_rust::<action::System>() {
                Ok(val) => {
                    map.insert(val, command);
                }
                Err(err) => {
                    tracing::warn!(
                        "Skipping over invalid SystemAction ({}): {}",
                        action.get_ron(),
                        err
                    );
                }
            };
        }

        Ok(SystemActionsImpl(map))
    }
}

impl<'de> Deserialize<'de> for SystemActionsImpl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        deserializer.deserialize_map(SystemActionsMapVisitor)
    }
}
