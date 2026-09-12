use crate::{config::ServerGameplayConfig, map::MapConfig};
use bevy::prelude::{Resource, Vec3};
use common::map::{CarrierPose, ZoneVolume};
use common::protocol::CarrierId;

#[derive(Clone)]
pub(crate) struct ActorTerritory {
    pub(crate) carrier: CarrierId,
    pub(crate) volume: ZoneVolume,
    pub(crate) distance: f32,
    pub(crate) center_height: f32,
}

impl ActorTerritory {
    pub(crate) fn contains_position(&self, point: Vec3) -> bool {
        self.volume
            .contains(point + Vec3::Y * self.center_height, self.distance)
    }

    pub(crate) fn path_contains(&self, from: Vec3, to: Vec3) -> bool {
        self.contains_position(from) && self.contains_position(to)
    }

    pub(crate) fn in_frame(&self, home: CarrierPose, frame: CarrierPose) -> Self {
        let mut territory = self.clone();
        territory.volume.min = frame.inverse_transform_point(home.transform_point(self.volume.min));
        territory.volume.max = frame.inverse_transform_point(home.transform_point(self.volume.max));
        territory
    }
}

#[derive(Clone, Default, Resource)]
pub struct ActorTerritories(Vec<ActorTerritory>);

impl ActorTerritories {
    pub fn new(map: &MapConfig, config: &ServerGameplayConfig) -> Self {
        let territories = map
            .actor_spawn_zones
            .iter()
            .map(|zone| {
                let kind = config.expect_actor(&zone.kind);
                let territory = ActorTerritory {
                    carrier: zone.carrier,
                    volume: zone.volume(map.grid(zone.carrier)),
                    distance: zone.roam_distance,
                    center_height: kind.character.physics().movement_collider.height / 2.0,
                };
                territory
            })
            .collect();
        Self(territories)
    }

    pub(crate) fn get(&self, zone_index: usize) -> &ActorTerritory {
        self.0
            .get(zone_index)
            .expect("actor spawn zone missing from territories")
    }
}
