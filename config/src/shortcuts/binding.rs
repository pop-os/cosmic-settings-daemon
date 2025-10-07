// SPDX-License-Identifier: MPL-2.0

use super::action::Direction;
use super::{Modifiers, ModifiersDef};
use serde::{Deserialize, Serialize};
use std::hash::Hash;
use std::str::FromStr;
use xkbcommon::xkb::{self, Keysym};

/// Description of a key combination that may be handled by the compositor
#[serde_with::serde_as]
#[derive(Clone, Debug, Default, Deserialize, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    /// What modifiers are expected to be pressed alongside the key
    #[serde_as(as = "serde_with::FromInto<ModifiersDef>")]
    pub modifiers: Modifiers,
    /// The actual key, that was pressed
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "super::sym::deserialize",
        serialize_with = "super::sym::serialize"
    )]
    pub key: Option<xkb::Keysym>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keycode: Option<u32>,
    // A custom description for a custom binding
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Binding {
    /// Creates a new key binding from a modifier and optional key
    pub fn new(modifiers: impl Into<Modifiers>, key: Option<xkb::Keysym>) -> Binding {
        Binding {
            description: None,
            modifiers: modifiers.into(),
            keycode: None,
            key,
        }
    }

    /// Creates a new key binding from a modifier and optional key
    pub fn new_keycode(modifiers: impl Into<Modifiers>, key: Option<xkb::Keycode>) -> Binding {
        Binding {
            description: None,
            modifiers: modifiers.into(),
            keycode: None,
            key: None,
        }
    }

    /// Creates a key binding from a string, where the non-modifier key is optional.
    pub fn from_str_partial(value: &str) -> Result<Self, String> {
        let mut binding = Binding::default();

        for token in value.split('+') {
            let token = token.trim();
            match token.to_ascii_lowercase().as_str() {
                "super" => binding.modifiers.logo = true,
                "ctrl" => binding.modifiers.ctrl = true,
                "alt" => binding.modifiers.alt = true,
                "shift" => binding.modifiers.shift = true,
                lowercased => {
                    if binding.key.is_some() {
                        return Err("only one non-modifier key is allowed".to_string());
                    }

                    let name = if token.chars().count() == 1 {
                        binding.key = Some(Keysym::from_char(lowercased.chars().next().unwrap()));
                        continue;
                    } else {
                        token
                    };

                    // Try case-sensitive lookup first in case of two symbols that only differ in case.
                    match xkb::keysym_from_name(&name, xkb::KEYSYM_NO_FLAGS) {
                        x if x.raw() == super::sym::NO_SYMBOL => {
                            // Fallback to case insensitive lookup.
                            match xkb::keysym_from_name(&name, xkb::KEYSYM_CASE_INSENSITIVE) {
                                x_insensitive if x_insensitive.raw() == super::sym::NO_SYMBOL => {
                                    return Err(format!("'{name}' is not a valid key symbol"));
                                }
                                x_insensitive => {
                                    binding.key = Some(x_insensitive);
                                }
                            }
                        }
                        x => {
                            binding.key = Some(x);
                        }
                    };
                }
            }
        }

        if !binding.has_modifier() && binding.key.is_none() {
            return Err("at least one key is required".to_string());
        }

        Ok(binding)
    }

    /// Check if a modifier was defined
    pub fn has_modifier(&self) -> bool {
        self.modifiers.logo || self.modifiers.shift || self.modifiers.alt || self.modifiers.ctrl
    }

    /// Check if the binding has been set
    pub fn is_set(&self) -> bool {
        (self.has_modifier() && self.key.is_some())
            || self.is_super()
            || self.key.map_or(false, |key| {
                // Allow Home/End, Print, PageDown/Up, etc.
                key.is_misc_function_key()
                    // XF86 keysym range
                    || matches!(key.raw(), 0x10080001..=0x1008FFFF)
            })
    }

    /// Check if the key binding is binding directly to Super
    pub fn is_super(&self) -> bool {
        self.key.is_none()
            && self.modifiers.logo
            && !self.modifiers.shift
            && !self.modifiers.alt
            && !self.modifiers.ctrl
    }

    /// Get the inferred direction of a xkb key
    pub fn inferred_direction(&self) -> Option<Direction> {
        match self.key? {
            xkb::Keysym::Left | xkb::Keysym::h | xkb::Keysym::H => Some(Direction::Left),
            xkb::Keysym::Down | xkb::Keysym::j | xkb::Keysym::J => Some(Direction::Down),
            xkb::Keysym::Up | xkb::Keysym::k | xkb::Keysym::K => Some(Direction::Up),
            xkb::Keysym::Right | xkb::Keysym::l | xkb::Keysym::L => Some(Direction::Right),
            _ => None,
        }
    }

    /// Append the binding to an existing string
    pub fn to_string_in_place(&self, string: &mut String) {
        if self.modifiers.logo {
            string.push_str("Super+");
        }

        if self.modifiers.ctrl {
            string.push_str("Ctrl+");
        }

        if self.modifiers.alt {
            string.push_str("Alt+");
        }

        if self.modifiers.shift {
            string.push_str("Shift+");
        }

        if let Some(key) = self.key {
            string.push_str(&xkb::keysym_get_name(key));
        } else if !string.is_empty() {
            string.remove(string.len() - 1);
        }
    }

    /// Returns `true` if the binding is a subset of another,
    /// i.e., `other` contains at least all the keys in `self`.
    pub fn is_subset(&self, other: &Binding) -> bool {
        (self.modifiers.ctrl & other.modifiers.ctrl == self.modifiers.ctrl)
            && (self.modifiers.alt & other.modifiers.alt == self.modifiers.alt)
            && (self.modifiers.shift & other.modifiers.shift == self.modifiers.shift)
            && (self.modifiers.logo & other.modifiers.logo == self.modifiers.logo)
            && (self.key.is_none() || self.key == other.key)
    }
}

impl PartialEq for Binding {
    fn eq(&self, other: &Self) -> bool {
        self.modifiers == other.modifiers && self.key == other.key
    }
}

impl ToString for Binding {
    fn to_string(&self) -> String {
        let mut string = String::new();
        self.to_string_in_place(&mut string);
        string
    }
}

impl Hash for Binding {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key.hash(state);
        self.modifiers.hash(state);
    }
}

impl FromStr for Binding {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let binding = Binding::from_str_partial(value)?;
        if binding.key.is_none() && !binding.modifiers.logo {
            return Err(format!("no key was defined for this binding"));
        }

        Ok(binding)
    }
}

fn uppercase_first_letter(input: &str) -> String {
    let mut chars = input.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::Binding;
    use crate::shortcuts::Modifiers;
    use std::str::FromStr;

    #[test]
    fn binding_from_str() {
        assert_eq!(
            Binding::from_str("Super+Q"),
            Ok(Binding::new(
                Modifiers::new().logo(),
                Some(xkbcommon::xkb::Keysym::from_char('q'))
            ))
        );

        assert_eq!(
            Binding::from_str("Super+Ctrl+Alt+F"),
            Ok(Binding::new(
                Modifiers::new().logo().ctrl().alt(),
                Some(xkbcommon::xkb::Keysym::from_char('f'))
            ))
        );

        assert_eq!(
            Binding::from_str("Super+Down"),
            Ok(Binding::new(
                Modifiers::new().logo(),
                Some(xkbcommon::xkb::Keysym::Down)
            ))
        );

        assert_eq!(
            Binding::from_str("XF86MonBrightnessDown"),
            Ok(Binding::new(
                Modifiers::new(),
                Some(xkbcommon::xkb::Keysym::XF86_MonBrightnessDown)
            ))
        );

        assert_eq!(
            Binding::from_str("Super+space"),
            Ok(Binding::new(
                Modifiers::new().logo(),
                Some(xkbcommon::xkb::Keysym::space)
            ))
        );

        // Case-insensitive
        assert_eq!(
            Binding::from_str("super+up"),
            Ok(Binding::new(
                Modifiers::new().logo(),
                Some(xkbcommon::xkb::Keysym::Up)
            ))
        );

        // Must have a non-modifier key.
        assert!(matches!(Binding::from_str("Super+Shift"), Err(_)));

        // Can't have multiple non-modifier keys.
        assert!(matches!(Binding::from_str("Super+Up+Down"), Err(_)));

        // At least one key is required.
        assert!(matches!(Binding::from_str(" "), Err(_)));
    }

    #[test]
    fn binding_from_str_partial() {
        // Non-modifier key not required.
        assert_eq!(
            Binding::from_str_partial("Super+Ctrl+Alt"),
            Ok(Binding::new(Modifiers::new().logo().ctrl().alt(), None,))
        );

        // Can't have multiple non-modifier keys.
        assert!(matches!(Binding::from_str("Super+Up+Down"), Err(_)));

        // At least one key is required.
        assert!(matches!(Binding::from_str(" "), Err(_)));
    }
}
