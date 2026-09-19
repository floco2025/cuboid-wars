use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

use super::{
    HexColor,
    kind_table::{KindId, KindTable},
};

// Index into the selected map's ordered `fields`: what a barrier or a light
// bridge belongs to, a key opens, and a switch turns on and off.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub struct FieldId(pub u16);

impl KindId for FieldId {
    // A full key inventory must fit in the PlayerStatus datagram.
    const MAX: Option<usize> = Some(256);
    const CONFIG_KEY: &'static str = "fields";
    const NOUN: &'static str = "field";

    fn from_index(index: u16) -> Self {
        Self(index)
    }

    fn index(self) -> u16 {
        self.0
    }
}

pub type FieldTable = KindTable<FieldId>;

// One entry of the root layout's `fields` catalog: a named force field with
// one state, as a switch is a named control with one state. Every barrier and
// light bridge naming it is a piece of it, solid while it is on, and its key
// lets the holder through all of them. `initially_on` is its state before any
// switch input, which `switch` flips while active; without one it keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDef {
    pub id: String,
    pub color: HexColor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub switch: Option<String>,
    #[serde(default = "initially_on", skip_serializing_if = "is_on")]
    pub initially_on: bool,
}

const fn initially_on() -> bool {
    true
}

const fn is_on(on: &bool) -> bool {
    *on
}

impl FieldTable {
    pub fn from_field_defs(defs: &[FieldDef]) -> anyhow::Result<Self> {
        Self::from_ids(defs.iter().map(|def| def.id.clone()).collect())
    }
}
