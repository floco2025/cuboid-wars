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
mod tests {
    use super::*;

    #[test]
    fn advancing_wraps_to_zero() {
        assert_eq!(PlayerGeneration(u32::MAX).next(), PlayerGeneration(0));
        assert_eq!(PlayerGeneration(6).next(), PlayerGeneration(7));
    }

    #[test]
    fn ordering_follows_advancement_across_the_wrap() {
        let last = PlayerGeneration(u32::MAX);
        let wrapped = last.next();
        assert!(wrapped.is_newer_than(last));
        assert!(!last.is_newer_than(wrapped));
        assert!(!wrapped.is_newer_than(wrapped));
        assert!(PlayerGeneration(3).is_newer_than(PlayerGeneration(2)));
        assert!(!PlayerGeneration(2).is_newer_than(PlayerGeneration(3)));
    }
}
