use bevy::prelude::*;

use crate::map::MapLevel;
use common::protocol::{CarrierId, MapLayout};

// The entity each carrier's map entities hang under, by carrier id (the
// world at 0). A carried entity keeps its carrier-local transform and rides
// its parent, which `carriers_transform_sync_system` moves.
#[derive(Resource)]
pub struct CarrierEntities(Vec<Entity>);

impl CarrierEntities {
    #[must_use]
    pub fn new(entities: Vec<Entity>) -> Self {
        Self(entities)
    }

    #[must_use]
    pub fn get(&self, id: CarrierId) -> Entity {
        *self
            .0
            .get(id.0 as usize)
            .expect("map record names a carrier the layout does not have")
    }
}

// Where each carrier's records sit in world storeys, by carrier id: the
// storey its local level 0 is on, and how many storeys above that its
// motion may reach, both summed up the parent chain.
#[derive(Resource)]
pub struct CarrierStoreys {
    base: Vec<u8>,
    motion: Vec<u8>,
}

impl CarrierStoreys {
    #[must_use]
    pub fn from_layout(layout: &MapLayout) -> Self {
        let ids = (0..=layout.carriers.len()).map(|index| CarrierId(index as u16));
        Self {
            base: ids.clone().map(|id| layout.carrier_base_level(id)).collect(),
            motion: ids.map(|id| layout.carrier_motion_levels(id)).collect(),
        }
    }

    // The level tag of a record on `carrier` at its local `level`, reaching
    // `span` storeys further by itself.
    #[must_use]
    pub fn tag(&self, carrier: CarrierId, level: u8, span: u8) -> MapLevel {
        let index = carrier.0 as usize;
        let base = self
            .base
            .get(index)
            .copied()
            .expect("map record names a carrier the layout does not have");
        let motion = self
            .motion
            .get(index)
            .copied()
            .expect("map record names a carrier the layout does not have");
        MapLevel {
            level: base.saturating_add(level),
            span: span.saturating_add(motion),
        }
    }
}

#[cfg(test)]
#[path = "tests/resources.rs"]
mod tests;
