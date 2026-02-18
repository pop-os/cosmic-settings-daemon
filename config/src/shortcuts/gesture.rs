// SPDX-License-Identifier: MPL-2.0
use std::{fmt::Display, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::action::{Direction, FingerCount};

/// Description of a gesture that can be handled by the compositor
#[serde_with::serde_as]
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize, Hash, Ord, PartialOrd)]
#[serde(deny_unknown_fields)]
pub struct Gesture {
    /// How many fingers are held down
    pub fingers: FingerCount,
    pub direction: Direction,
    // A custom description for a custom binding
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Gesture {
    /// Creates a new gesture from a number of fingers and a direction
    pub fn new(fingers: FingerCount, direction: Direction) -> Gesture {
        Gesture {
            fingers,
            direction,
            description: None,
        }
    }
}

impl Display for Gesture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} Finger {}",
            <&'static str>::from(self.fingers),
            <&'static str>::from(self.direction)
        )
    }
}

#[derive(Error, Debug, PartialEq)]
pub enum GestureParseError {
    #[error("Expected value for the number of fingers")]
    NoFingerValue,
    #[error("Invalid finger value `{0}`")]
    InvalidFingerValue(String),
    #[error("Expected value for the direction")]
    NoDirectionValue,
    #[error("Invalid direction value `{0}`")]
    InvalidDirectionValue(String),
    #[error("Received unknown extra data `{0}`")]
    ExtraData(String),
}

impl FromStr for Gesture {
    type Err = GestureParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut value_iter = value.split("+");

        let Some(n) = value_iter.next() else {
            return Err(GestureParseError::NoFingerValue);
        };
        let Ok(fingers) = FingerCount::from_str(n) else {
            return Err(GestureParseError::InvalidFingerValue(n.to_string()));
        };

        let Some(n2) = value_iter.next() else {
            return Err(GestureParseError::NoDirectionValue);
        };

        let Ok(direction) = Direction::from_str(n2) else {
            return Err(GestureParseError::InvalidDirectionValue(n2.to_string()));
        };

        if let Some(n3) = value_iter.next() {
            return Err(GestureParseError::ExtraData(n3.to_string()));
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

    use crate::shortcuts::action::{Direction, FingerCount};

    use super::Gesture;
    use std::str::FromStr;

    #[test]
    fn binding_from_str() {
        assert_eq!(
            Gesture::from_str("3+Left"),
            Ok(Gesture::new(FingerCount::Three, Direction::Left))
        );

        assert_eq!(
            Gesture::from_str("5+Up"),
            Ok(Gesture::new(FingerCount::Five, Direction::Up))
        );

        assert_ne!(
            Gesture::from_str("4+Left+More+Info"),
            Ok(Gesture::new(FingerCount::Four, Direction::Left))
        );
    }
}
