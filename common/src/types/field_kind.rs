use bincode::{Decode, Encode};

use super::kind_table::{KindId, KindTable};

// Index into the selected map's ordered `field_kinds`, shared by barriers,
// light bridges, and the keys that pass them.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub struct FieldKindId(pub u16);

impl KindId for FieldKindId {
    // A full key inventory must fit in the PlayerStatus datagram.
    const MAX: Option<usize> = Some(256);
    const CONFIG_KEY: &'static str = "field_kinds";
    const NOUN: &'static str = "field kind";

    fn from_index(index: u16) -> Self {
        Self(index)
    }

    fn index(self) -> u16 {
        self.0
    }
}

pub type FieldKindTable = KindTable<FieldKindId>;
