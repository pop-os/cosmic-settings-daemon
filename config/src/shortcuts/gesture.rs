// SPDX-License-Identifier: MPL-2.0
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::action::Direction;

/// Description of a gesture that can be handled by the compositor
#[serde_with::serde_as]
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize, Hash)]
#[serde(deny_unknown_fields)]
pub struct Gesture {
    /// How many fingers are held down
    pub fingers: u32,
    pub direction: Direction,
    // A custom description for a custom binding
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Gesture {
    /// Creates a new gesture from a number of fingers and a direction
    pub fn new(fingers: impl Into<u32>, direction: impl Into<Direction>) -> Gesture {
        Gesture {
            fingers: fingers.into(),
            direction: direction.into(),
            description: None,
        }
    }

    /// Append the binding to an existing string
    pub fn to_string_in_place(&self, string: &mut String) {
        string.push_str(&format!(
            "{} Finger {}",
            self.fingers,
            self.direction.to_string()
        ));
    }
}

impl ToString for Gesture {
    fn to_string(&self) -> String {
        let mut string = String::new();
        self.to_string_in_place(&mut string);
        string
    }
}

impl FromStr for Gesture {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut value_iter = value.split("+");
        let n = match value_iter.next() {
            Some(val) => val,
            None => {
                return Err(format!("no value for the number of fingers"));
            }
        };
        let fingers = match u32::from_str(n) {
            Ok(a) => a,
            Err(_) => {
                return Err(format!("could not parse number of fingers"));
            }
        };

        let n2 = match value_iter.next() {
            Some(val) => val,
            None => {
                return Err(format!("could not parse direction"));
            }
        };

        let direction = match Direction::from_str(n2) {
            Ok(dir) => dir,
            Err(e) => {
                return Err(e);
            }
        };

        if let Some(n3) = value_iter.next() {
            return Err(format!("Extra data {} not expected", n3));
        }

        return Ok(Self {
            fingers,
            direction,
            description: None,
        });
    }
}

#[cfg(test)]
mod tests {

    use crate::shortcuts::action::Direction;

    use super::Gesture;
    use std::str::FromStr;

    #[test]
    fn binding_from_str() {
        assert_eq!(
            Gesture::from_str("3+Left"),
            Ok(Gesture::new(
                3 as u32,
                Direction::Left
            ))
        );

        assert_eq!(
            Gesture::from_str("5+Up"),
            Ok(Gesture::new(
                5 as u32,
                Direction::Up
            ))
        );

        assert_ne!(
            Gesture::from_str("4+Left+More+Info"),
            Ok(Gesture::new(
                4 as u32,
                Direction::Left
            ))
        );
    }
}
