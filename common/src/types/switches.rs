use bincode::{Decode, Encode};
use serde::Deserialize;

use super::kind_table::{KindId, KindTable};
use crate::config::PressureSwitchConfig;

// Index into the selected map's ordered `switches`: what a pressure plate
// operates, and what barrier kinds, bridge kinds, carriers, actor spawn
// zones, and the fireworks name to be driven by it.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub struct SwitchId(pub u16);

impl KindId for SwitchId {
    const MAX: Option<usize> = None;
    const CONFIG_KEY: &'static str = "switches";
    const NOUN: &'static str = "switch";

    fn from_index(index: u16) -> Self {
        Self(index)
    }

    fn index(self) -> u16 {
        self.0
    }
}

pub type SwitchTable = KindTable<SwitchId>;

// One entry of a map's `switches` catalog.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Deserialize)]
pub struct SwitchDef {
    pub id: String,
    #[serde(flatten)]
    pub policy: PressureSwitchConfig,
}

impl SwitchTable {
    pub fn from_switch_defs(defs: &[SwitchDef]) -> anyhow::Result<Self> {
        Self::from_ids(defs.iter().map(|def| def.id.clone()).collect())
    }
}
