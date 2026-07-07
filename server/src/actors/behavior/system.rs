use bevy::prelude::*;
use rand::rng;

use crate::{
    actors::navigation::NavGraph,
    config::ServerGameplayConfig,
    resources::{ActorMap, MapConfig, PlayerMap},
};
use common::{
    config::GameplayConfig,
    map_geometry::MapGeometry,
    physics::CollisionWorld,
    protocol::{ActorId, ActorMarker, PlayerId, PlayerMarker, Position},
};

use super::{
    perception::visible_player_position,
    tick::{BehaviorInputs, active_leash, patrolling_off_zone_level, tick_actor_behavior},
    zone::xz_distance_from_rect,
};

// Thin ECS shell: gathers plain-data inputs (configs, leash geometry, gated
// perception) and hands the entire per-actor decision to
// `tick_actor_behavior`, which owns every `ActorGoal` transition.
pub fn actor_behavior_system(
    time: Res<Time>,
    players: Res<PlayerMap>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    map_config: Res<MapConfig>,
    map_geometry: Res<MapGeometry>,
    nav_graph: Res<NavGraph>,
    mut actors: ResMut<ActorMap>,
    player_query: Query<(&PlayerId, &Position), With<PlayerMarker>>,
    query: Query<(&ActorId, &Position), (With<ActorMarker>, Without<PlayerMarker>)>,
) {
    let delta = time.delta_secs();
    let mut rng = rng();

    for (id, pos) in &query {
        let Some(info) = actors.get_mut(id) else {
            continue;
        };
        // Per-kind config: actor kinds are cross-validated against the map at
        // startup, so `validated_actor` cannot panic here.
        let actor_config = gameplay_config.validated_actor(&info.spawn_kind);
        let kind_config = server_gameplay_config.validated_actor(&info.spawn_kind);

        let zone = &map_config.actor_spawn_zones[info.spawn_zone_index];
        let zone_bounds = zone.xz_bounds(&map_geometry);
        let beyond_leash = xz_distance_from_rect(pos, zone_bounds) > active_leash(&info.goal, kind_config)
            || patrolling_off_zone_level(&info.goal, pos.y, zone.level);
        // Perception is skipped beyond the leash — the actor is heading home
        // regardless.
        let visible_player = if beyond_leash {
            None
        } else {
            visible_player_position(
                pos,
                actor_config.eye_height(),
                kind_config.senses.horizontal_vision_range,
                kind_config.senses.vertical_vision_range,
                &players,
                &player_query,
                &collision_world,
                &gameplay_config,
            )
        };

        let inputs = BehaviorInputs {
            pos: *pos,
            delta,
            beyond_leash,
            visible_player,
            zone,
            zone_bounds,
            nav_graph: &nav_graph,
            patrol_speed: actor_config.patrol_speed,
            kind_config,
        };
        tick_actor_behavior(info, &inputs, &mut rng);
    }
}
