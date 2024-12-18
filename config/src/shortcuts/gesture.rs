// SPDX-License-Identifier: MPL-2.0
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Description of a gesture that can be handled by the compositor
#[serde_with::serde_as]
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize, Hash)]
#[serde(deny_unknown_fields)]
pub struct Gesture {
    /// How many fingers are held down
    pub fingers: i32,
    pub direction: Direction,
    // A custom description for a custom binding
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Describes a direction, either absolute or relative
#[serde_with::serde_as]
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize, Hash)]
#[serde(deny_unknown_fields)]
pub enum Direction {
    Relative(RelativeDirection),
    Absolute(AbsoluteDirection),
}

impl ToString for Direction {
    fn to_string(&self) -> String {
        match self {
            Direction::Absolute(abs) => match abs {
                AbsoluteDirection::AbsoluteUp => "AbsoluteUp".to_string(),
                AbsoluteDirection::AbsoluteDown => "AbsoluteDown".to_string(),
                AbsoluteDirection::AbsoluteLeft => "AbsoluteLeft".to_string(),
                AbsoluteDirection::AbsoluteRight => "AbsoluteRight".to_string(),
            },
            Direction::Relative(rel) => match rel {
                RelativeDirection::RelativeForward => "RelativeForward".to_string(),
                RelativeDirection::RelativeBackward => "RelativeBackward".to_string(),
                RelativeDirection::RelativeLeft => "RelativeLeft".to_string(),
                RelativeDirection::RelativeRight => "RelativeRight".to_string(),
            },
        }
    }
}

impl FromStr for Direction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            // Absolute directions
            "AbsoluteUp" => Ok(Direction::Absolute(AbsoluteDirection::AbsoluteUp)),
            "AbsoluteDown" => Ok(Direction::Absolute(AbsoluteDirection::AbsoluteDown)),
            "AbsoluteLeft" => Ok(Direction::Absolute(AbsoluteDirection::AbsoluteLeft)),
            "AbsoluteRight" => Ok(Direction::Absolute(AbsoluteDirection::AbsoluteRight)),
            // Relative directions
            "RelativeForward" => Ok(Direction::Relative(RelativeDirection::RelativeForward)),
            "RelativeBackward" => Ok(Direction::Relative(RelativeDirection::RelativeBackward)),
            "RelativeLeft" => Ok(Direction::Relative(RelativeDirection::RelativeLeft)),
            "RelativeRight" => Ok(Direction::Relative(RelativeDirection::RelativeRight)),
            _ => Err(format!("Invalid direction string"))
        }
    }
}

/// Describes a relative direction (typically relative to the workspace direction)
#[serde_with::serde_as]
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize, Hash)]
#[serde(deny_unknown_fields)]
pub enum RelativeDirection {
    RelativeForward,
    RelativeBackward,
    RelativeLeft,
    RelativeRight,
}

/// Describes an absolute direction (i.e. not relative to workspace direction)
#[serde_with::serde_as]
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize, Hash)]
#[serde(deny_unknown_fields)]
pub enum AbsoluteDirection {
    AbsoluteUp,
    AbsoluteDown,
    AbsoluteLeft,
    AbsoluteRight,
}

impl Gesture {
    /// Creates a new gesture from a number of fingers and a direction
    pub fn new(fingers: impl Into<i32>, direction: impl Into<Direction>) -> Gesture {
        Gesture {
            fingers: fingers.into(),
            direction: direction.into(),
            description: None,
        }
    }

    /// Returns true if the direction is absolute
    pub fn is_absolute(&self) -> bool {
        matches!(self.direction, Direction::Absolute(_))
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
            },
        };
        let fingers = match i32::from_str(n) {
            Ok(a) => a,
            Err(_) => {
                return Err(format!("could not parse number of fingers"));
            },
        };

        let n2 = match value_iter.next() {
            Some(val) => val,
            None => {
                return Err(format!("could not parse direction"));
            },
        };

        let direction = match Direction::from_str(n2) {
            Ok(dir) => dir,
            Err(e) => {
                return Err(e);
            },
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
    use crate::shortcuts::gesture::{AbsoluteDirection, Direction, RelativeDirection};

    use super::Gesture;
    use std::str::FromStr;

    #[test]
    fn binding_from_str() {
        assert_eq!(
            Gesture::from_str("3+RelativeLeft"),
            Ok(Gesture::new(
                3,
                Direction::Relative(RelativeDirection::RelativeLeft)
            ))
        );

        assert_eq!(
            Gesture::from_str("5+AbsoluteUp"),
            Ok(Gesture::new(
                5,
                Direction::Absolute(AbsoluteDirection::AbsoluteUp)
            ))
        );

        assert_ne!(
            Gesture::from_str("4+AbsoluteLeft+More+Info"),
            Ok(Gesture::new(
                4,
                Direction::Absolute(AbsoluteDirection::AbsoluteLeft)
            ))
        );
    }
}
