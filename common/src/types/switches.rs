use bincode::{Decode, Encode};
use serde::Deserialize;

use super::{
    HexColor, MapLayout, MapSettings,
    kind_table::{KindId, KindTable},
};
use crate::config::PressureSwitchConfig;

// Index into the root layout's pressure plate kind catalog, shared by plates and targets.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub struct SwitchId(pub u16);

impl KindId for SwitchId {
    const MAX: Option<usize> = None;
    const CONFIG_KEY: &'static str = "switch_kinds";
    const NOUN: &'static str = "switch";

    fn from_index(index: u16) -> Self {
        Self(index)
    }

    fn index(self) -> u16 {
        self.0
    }
}

pub type SwitchTable = KindTable<SwitchId>;

// One entry of the root layout's `switch_kinds` catalog.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Deserialize)]
pub struct SwitchDef {
    pub id: String,
    #[serde(default)]
    pub plate_color: Option<HexColor>,
    #[serde(flatten)]
    pub policy: PressureSwitchConfig,
}

impl MapSettings {
    pub fn pressure_plate_color(&self, switch: SwitchId, layout: &MapLayout) -> Option<HexColor> {
        let def = self.switches.get(usize::from(switch.0))?;
        def.plate_color.or_else(|| {
            self.barrier_kinds
                .iter()
                .enumerate()
                .find(|(index, _)| {
                    layout
                        .barriers
                        .iter()
                        .any(|b| usize::from(b.kind.0) == *index && b.switch == Some(switch))
                })
                .map(|(_, kind)| kind.color)
                .or_else(|| {
                    self.bridge_kinds
                        .iter()
                        .enumerate()
                        .find(|(index, _)| {
                            layout
                                .light_bridges
                                .iter()
                                .any(|b| usize::from(b.kind.0) == *index && b.switch == Some(switch))
                        })
                        .map(|(_, kind)| kind.color)
                })
        })
    }
}

impl SwitchTable {
    pub fn from_switch_defs(defs: &[SwitchDef]) -> anyhow::Result<Self> {
        Self::from_ids(defs.iter().map(|def| def.id.clone()).collect())
    }
}
