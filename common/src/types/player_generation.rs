use bincode::{Decode, Encode};

use crate::math::sequence_is_newer;

// Identifies a player's current body; respawns and forced relocations advance it
// so delayed messages cannot affect the replacement body.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Encode, Decode)]
pub struct PlayerGeneration(pub u32);

impl PlayerGeneration {
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    #[must_use]
    pub const fn is_newer_than(self, other: Self) -> bool {
        sequence_is_newer(self.0, other.0)
    }
}

#[cfg(test)]
#[path = "tests/player_generation.rs"]
mod tests;
