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
    fn wire_encoding_matches_the_integer_without_extra_bytes() {
        for value in [0, 250, 251, 65_535, 65_536, u32::MAX] {
            let config = bincode::config::standard();
            let bytes = bincode::encode_to_vec(PlayerGeneration(value), config).expect("generation encoding failed");
            assert_eq!(
                bytes,
                bincode::encode_to_vec(value, config).expect("integer encoding failed")
            );
            let (decoded, length): (PlayerGeneration, _) =
                bincode::decode_from_slice(&bytes, config).expect("generation decoding failed");
            assert_eq!(decoded, PlayerGeneration(value));
            assert_eq!(length, bytes.len());
        }
    }
}
