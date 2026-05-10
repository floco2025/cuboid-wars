use bevy::prelude::*;
use rand::rng;

use crate::{
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
    patrol::{random_direction_time, random_patrol_intent},
    perception::visible_player_position,
    zone::{closest_point_in_rect, xz_distance_from_rect},
};

pub fn actor_behavior_system(
    time: Res<Time>,
    players: Res<PlayerMap>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    map_config: Res<MapConfig>,
    map_geometry: Res<MapGeometry>,
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
        // Per-kind config: cross-validated against the map at startup, so unwraps are safe.
        let actor_config = gameplay_config.validated_actor(&info.spawn_kind);
        let kind_server_config = server_gameplay_config.validated_actor(&info.spawn_kind);

        // Wander limit: if the actor has strayed past `max_wander_distance`
        // from the nearest edge of its spawn zone, override everything else
        // and walk it back. This naturally cancels chases that carry the
        // actor too far — the next tick rewrites `go_to_position` from the
        // player's location to a point inside the zone.
        let zone_bounds = map_config.actor_spawn_zones[info.spawn_zone_index].xz_bounds(&map_geometry);
        if xz_distance_from_rect(pos, zone_bounds) > kind_server_config.max_wander_distance {
            info.go_to_position = Some(closest_point_in_rect(pos, zone_bounds));
            continue;
        }

        if let Some(target_pos) = visible_player_position(
            pos,
            actor_config.eye_height(),
            kind_server_config.vision_range,
            &players,
            &player_query,
            &collision_world,
            &gameplay_config,
        ) {
            info.go_to_position = Some(target_pos);
            continue;
        }

        if info.go_to_position.is_some() {
            continue;
        }
        info.direction_timer -= delta;
        if info.direction_timer > 0.0 {
            continue;
        }

        info.direction_timer = random_direction_time(&mut rng, kind_server_config);
        info.patrol_intent =
            random_patrol_intent(&mut rng, actor_config.patrol_speed, kind_server_config.idle_probability);
    }
}
