use bincode::{Decode, Encode};
use serde::Deserialize;

use super::DeathTrigger;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Encode, Decode, Deserialize)]
pub struct PressureSwitchConfig {
    pub activation: PressureSwitchActivation,
    pub reset_on_player_death: DeathTrigger,
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
