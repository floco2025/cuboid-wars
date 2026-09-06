use bevy::prelude::*;

use super::CarrierEntities;
use common::{
    map::Carriers,
    protocol::{CarrierId, MapLayout},
};

// The root entity of one carrier's map entities.
#[derive(Component)]
pub struct CarrierMarker {
    pub id: CarrierId,
}

// One root entity per carrier, the world included, at its pose at the
// current tick; spawned once at bootstrap since the layout never changes.
pub fn spawn_carrier_entities(world: &mut World, layout: &MapLayout, carriers: &Carriers) -> CarrierEntities {
    let entities = (0..=layout.carriers.len())
        .map(|index| {
            let id = CarrierId(index as u16);
            world
                .spawn((
                    CarrierMarker { id },
                    Transform::from_translation(carriers.pose(id).translation),
                    Visibility::default(),
                ))
                .id()
        })
        .collect();
    CarrierEntities::new(entities)
}
