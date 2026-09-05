use bincode::{Decode, Encode};

use super::kind_table::{KindId, KindTable};

// Index into the selected map's ordered `barrier_kinds`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub struct BarrierKindId(pub u16);

impl KindId for BarrierKindId {
    const MAX: Option<usize> = Some(27);
    const CONFIG_KEY: &'static str = "barrier_kinds";
    const NOUN: &'static str = "barrier";

    fn from_index(index: u16) -> Self {
        Self(index)
    }

    fn index(self) -> u16 {
        self.0
    }
}

pub type BarrierKindTable = KindTable<BarrierKindId>;
