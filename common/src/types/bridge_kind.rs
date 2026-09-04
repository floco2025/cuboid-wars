use bincode::{Decode, Encode};

use super::kind_table::{KindId, KindTable};

// Index into the selected map's ordered `bridge_kinds`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub struct BridgeKindId(pub u16);

impl KindId for BridgeKindId {
    const MAX: Option<usize> = None;
    const CONFIG_KEY: &'static str = "bridge_kinds";
    const NOUN: &'static str = "bridge";

    fn from_index(index: u16) -> Self {
        Self(index)
    }

    fn index(self) -> u16 {
        self.0
    }
}

pub type BridgeKindTable = KindTable<BridgeKindId>;
