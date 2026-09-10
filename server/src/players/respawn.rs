use bevy::prelude::*;

use super::{PlayerMap, place_player_body, player_spawn_destination};
use crate::{
    actors::{ActorMap, ActorRespawnTimers, PendingActorSpawns, reset_actors},
    config::ServerGameplayConfig,
    map::MapConfig,
    portals::PortalAssignments,
};
use common::{
    config::GameplayConfig,
    map::Carriers,
    physics::CollisionWorld,
    protocol::{Health, MapLayout, PlayerMarker, Position, ServerTick},
};

// Individual timers and the shared group timer expire here, after combat.
// Actor resets wait through the respawn delay so kills during the countdown are included;
// logout keeps any remaining countdown or starts the same delay, even on an empty server.
// Replacements use the normal beam-in warning. Shots remain in flight.
// Players get fresh bodies with full health; death already cleared per-life state.
//
// The reliable relocation establishes the body before any impulses for it.
pub fn players_respawn_system(
    mut commands: Commands,
    time: Res<Time>,
    tick: Res<ServerTick>,
    portal_assignments: Res<PortalAssignments>,
    mut players: ResMut<PlayerMap>,
    map_config: Res<MapConfig>,
    map_layout: Res<MapLayout>,
    carriers: Res<Carriers>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    player_query: Query<&Position, With<PlayerMarker>>,
    mut actors: ResMut<ActorMap>,
    mut actor_timers: ResMut<ActorRespawnTimers>,
    mut pending_actors: ResMut<PendingActorSpawns>,
) {
    let (to_respawn, actor_scope) = players.tick_respawns(time.delta_secs());
    if let Some(scope) = actor_scope {
        reset_actors(
            &mut commands,
            &mut actors,
            &mut pending_actors,
            &mut actor_timers,
            &map_config,
            scope,
        );
    }

    if to_respawn.is_empty() {
        return;
    }

    let mut occupied_positions: Vec<Position> = player_query.iter().copied().collect();
    for id in to_respawn {
        let saved = players.get(&id).and_then(|player| player.session.checkpoint);
        let Some(spawn) = player_spawn_destination(
            &map_config,
            &map_layout.checkpoints,
            &carriers,
            &collision_world,
            &occupied_positions,
            gameplay_config.player.physics(),
            saved,
        ) else {
            continue;
        };
        let entity = commands.spawn((PlayerMarker, id)).id();
        if let Some(info) = players.get_mut(&id) {
            info.finish_respawn(entity);
        }
        place_player_body(
            &mut commands,
            &mut players,
            id,
            entity,
            &spawn,
            Health(server_gameplay_config.combat.health.player.max),
            tick.0,
            portal_assignments.get(&id),
        );
        occupied_positions.push(spawn.pos);
        info!("{} respawned at {:?}", players.describe(&id), spawn.pos);
    }
}
