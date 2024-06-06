// SPDX-License-Identifier: MPL-2.0

use serde::Deserialize;
use xkbcommon::xkb;

// From x11rb, used to fill unused keysym table entries.
const NO_SYMBOL: u32 = 0;

#[allow(non_snake_case)]
pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<xkb::Keysym>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Error, Unexpected};

    let name = String::deserialize(deserializer)?;

    match xkb::keysym_from_name(&name, xkb::KEYSYM_NO_FLAGS) {
        x if x.raw() == NO_SYMBOL => {
            match xkb::keysym_from_name(&name, xkb::KEYSYM_CASE_INSENSITIVE) {
                x if x.raw() == NO_SYMBOL => Err(<D::Error as Error>::invalid_value(
                    Unexpected::Str(&name),
                    &"One of the keysym names of xkbcommon.h without the 'KEY_' prefix",
                )),
                x => {
                    tracing::warn!(
                        "Key-Binding '{}' only matched case insensitive for {:?}",
                        name,
                        xkb::keysym_get_name(x)
                    );
                    Ok(Some(x))
                }
            }
        }
        x => Ok(Some(x)),
    }
}

#[allow(non_snake_case)]
pub fn serialize<S: serde::Serializer>(
    key: &Option<xkb::Keysym>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let keysym;

    serializer.serialize_str(match key {
        Some(key) => {
            keysym = xkb::keysym_get_name(*key);
            &keysym
        }

        None => "None",
    })
}
