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
        let base = self.base.get(index).copied();
        let motion = self.motion.get(index).copied();
        let (Some(base), Some(motion)) = (base, motion) else {
            panic!("map record names a carrier the layout does not have");
        };
        MapLevel {
            level: base.saturating_add(level),
            span: span.saturating_add(motion),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::protocol::{Carrier, Position};

    #[test]
    fn tag_adds_the_carrier_base_and_motion() {
        let lift = Carrier {
            parent: CarrierId::WORLD,
            level: 2,
            levels: 1,
            from: Position::default(),
            to: Position::default(),
            travel_ticks: 1,
            pause_ticks: 0,
            phase_ticks: 0,
        };
        let storeys = CarrierStoreys::from_layout(&MapLayout {
            carriers: vec![lift],
            ..Default::default()
        });

        assert_eq!(storeys.tag(CarrierId::WORLD, 1, 0), MapLevel { level: 1, span: 0 });
        assert_eq!(storeys.tag(CarrierId(1), 0, 0), MapLevel { level: 2, span: 1 });
        assert_eq!(storeys.tag(CarrierId(1), 1, 2), MapLevel { level: 3, span: 3 });
    }
}
