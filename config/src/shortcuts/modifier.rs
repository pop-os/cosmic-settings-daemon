// SPDX-License-Identifier: MPL-2.0

use serde::{Deserialize, Serialize};
#[cfg(feature = "smithay")]
use smithay::input::keyboard::ModifiersState;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Super,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub logo: bool,
}

impl Modifiers {
    pub const fn new() -> Self {
        Self {
            ctrl: false,
            alt: false,
            shift: false,
            logo: false,
        }
    }

    pub const fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    pub const fn alt(mut self) -> Self {
        self.alt = true;
        self
    }

    pub const fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    pub const fn logo(mut self) -> Self {
        self.logo = true;
        self
    }
}

#[cfg(feature = "smithay")]
impl PartialEq<ModifiersState> for Modifiers {
    fn eq(&self, other: &ModifiersState) -> bool {
        self.ctrl == other.ctrl
            && self.alt == other.alt
            && self.shift == other.shift
            && self.logo == other.logo
    }
}

#[cfg(feature = "smithay")]
impl Into<Modifiers> for ModifiersState {
    fn into(self) -> Modifiers {
        Modifiers {
            ctrl: self.ctrl,
            alt: self.alt,
            shift: self.shift,
            logo: self.logo,
        }
    }
}

impl std::ops::AddAssign<Modifier> for Modifiers {
    fn add_assign(&mut self, rhs: Modifier) {
        match rhs {
            Modifier::Ctrl => self.ctrl = true,
            Modifier::Alt => self.alt = true,
            Modifier::Shift => self.shift = true,
            Modifier::Super => self.logo = true,
        };
    }
}

impl std::ops::BitOr for Modifier {
    type Output = Modifiers;

    fn bitor(self, rhs: Modifier) -> Self::Output {
        let mut modifiers = self.into();
        modifiers += rhs;
        modifiers
    }
}

impl Into<Modifiers> for Modifier {
    fn into(self) -> Modifiers {
        let mut modifiers = Modifiers {
            ctrl: false,
            alt: false,
            shift: false,
            logo: false,
        };
        modifiers += self;
        modifiers
    }
}

#[derive(Deserialize, Serialize)]
#[serde(transparent)]
pub struct ModifiersDef(Vec<Modifier>);

impl From<Modifiers> for ModifiersDef {
    fn from(src: Modifiers) -> Self {
        let mut modifiers = Vec::new();

        if src.logo {
            modifiers.push(Modifier::Super)
        }

        if src.ctrl {
            modifiers.push(Modifier::Ctrl);
        }

        if src.alt {
            modifiers.push(Modifier::Alt);
        }

        if src.shift {
            modifiers.push(Modifier::Shift)
        }

        Self(modifiers)
    }
}

impl From<ModifiersDef> for Modifiers {
    fn from(src: ModifiersDef) -> Self {
        src.0.into_iter().fold(
            Modifiers {
                ctrl: false,
                alt: false,
                shift: false,
                logo: false,
            },
            |mut modis, modi: Modifier| {
                modis += modi;
                modis
            },
        )
    }
}
