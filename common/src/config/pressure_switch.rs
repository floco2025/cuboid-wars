use bincode::{Decode, Encode};
use serde::Deserialize;

use super::DeathTrigger;

// One switch's policy: how its plates activate it, what holds it, and which
// player deaths reset a toggle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Encode, Decode, Deserialize)]
pub struct PressureSwitchConfig {
    pub activation: PressureSwitchActivation,
    pub reset_on_player_death: DeathTrigger,
    #[serde(default)]
    pub held: SwitchHold,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Encode, Decode, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PressureSwitchActivation {
    Momentary,
    Toggle,
    #[default]
    Auto,
}

impl PressureSwitchActivation {
    pub fn is_toggle(self, logged_in: usize) -> bool {
        match self {
            Self::Momentary => false,
            Self::Toggle => true,
            Self::Auto => logged_in == 1,
        }
    }
}

// What counts as the switch's plates being held: any one occupied plate, or
// every living player on one of them (every plate when players outnumber
// them).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Encode, Decode, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwitchHold {
    #[default]
    Any,
    Everyone,
}

impl SwitchHold {
    // `plates` and `held` count this switch's plates; `alive` the living
    // players.
    #[must_use]
    pub fn is_held(self, plates: usize, held: usize, alive: usize) -> bool {
        match self {
            Self::Any => held > 0,
            Self::Everyone => plates > 0 && alive > 0 && held >= plates.min(alive),
        }
    }
}

#[cfg(test)]
#[path = "tests/pressure_switch.rs"]
mod tests;
