use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

use super::{
    HexColor, MapSettings,
    kind_table::{KindId, KindTable},
};
use crate::config::SwitchConfig;

// Index into the root layout's switch catalog, shared by plates and targets.
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

// One entry of the root layout's `switches` catalog.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Deserialize, Serialize)]
pub struct SwitchDef {
    pub id: String,
    #[serde(default)]
    pub color: Option<HexColor>,
    #[serde(flatten)]
    pub policy: SwitchConfig,
}

impl MapSettings {
    // An authored colour, else that of the first field in catalog order the switch drives.
    pub fn switch_color(&self, switch: SwitchId) -> Option<HexColor> {
        let def = self.switches.get(usize::from(switch.0))?;
        def.color.or_else(|| {
            self.fields
                .iter()
                .find(|field| field.switch.as_deref() == Some(def.id.as_str()))
                .map(|field| field.color)
        })
    }
}

impl SwitchTable {
    pub fn from_switch_defs(defs: &[SwitchDef]) -> anyhow::Result<Self> {
        Self::from_ids(defs.iter().map(|def| def.id.clone()).collect())
    }
}
