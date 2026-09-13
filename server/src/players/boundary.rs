use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    map::{BoundaryTimer, Carriers},
    physics::CollisionWorld,
    protocol::{Health, MapLayout, PlayerId, PlayerMarker, Position, ServerTick},
};

use super::{PlayerMap, place_player_body, player_spawn_destination, spawn_zone_destination};
use crate::{map::MapConfig, portals::PortalAssignments};

pub(super) fn players_boundary_system(
    mut commands: Commands,
    time: Res<Time>,
    tick: Res<ServerTick>,
    layout: Res<MapLayout>,
    map: Res<MapConfig>,
    carriers: Res<Carriers>,
    collision: Res<CollisionWorld>,
    gameplay: Res<GameplayConfig>,
    portals: Res<PortalAssignments>,
    mut players: ResMut<PlayerMap>,
    bodies: Query<(Entity, &PlayerId, &Position, &Health), With<PlayerMarker>>,
) {
    let Some(grounds) = &layout.grounds else { return };
    let mut returned = Vec::new();
    for (entity, id, pos, health) in &bodies {
        let Some(player) = players.get_mut(id).filter(|player| !player.is_dead()) else {
            continue;
        };
        let outside = grounds.outside_boundary((*pos).into());
        let mut timer = BoundaryTimer(player.life.boundary_elapsed_secs);
        let remaining = timer.tick(outside, time.delta_secs(), grounds.settings.return_secs);
        player.life.boundary_elapsed_secs = timer.0;
        let beyond_terrain = grounds.distance_outside_map(pos.x, pos.z) > 380.0;
        if remaining != Some(0.0) && !beyond_terrain {
            continue;
        }
        let saved = player.session.checkpoint;
        let occupied: Vec<_> = bodies
            .iter()
            .filter(|(_, other, _, _)| *other != id)
            .map(|(_, _, pos, _)| *pos)
            .chain(returned.iter().copied())
            .collect();
        let physics = gameplay.player.physics();
        let spawn = player_spawn_destination(
            &map,
            &layout.checkpoints,
            &carriers,
            &collision,
            &occupied,
            physics,
            saved,
        )
        .unwrap_or_else(|| {
            spawn_zone_destination(&map, &layout.checkpoints, &carriers, &collision, &occupied, physics)
        });
        players.get_mut(id).expect("boundary player missing").advance_body();
        place_player_body(
            &mut commands,
            &mut players,
            *id,
            entity,
            &spawn,
            *health,
            tick.0,
            portals.get(id),
        );
        returned.push(spawn.pos);
    }
}

#[cfg(test)]
#[path = "tests/boundary.rs"]
mod tests;
