use bevy::prelude::*;
use rand::rng;

use crate::{
    config::ServerGameplayConfig,
    nav::NavGraph,
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
        // Per-kind config: cross-validated against the map at startup, so unwraps are safe.
        let actor_config = gameplay_config.validated_actor(&info.spawn_kind);
        let kind_server_config = server_gameplay_config.validated_actor(&info.spawn_kind);
        let chase_reacquire_blocked = tick_chase_reacquire_timer(info, delta);

        // Wander limit: if the actor has strayed past `max_wander_distance`
        // from the nearest edge of its spawn zone, override everything else,
        // start the chase reacquire cooldown when applicable, and walk it back.
        let zone_bounds = map_config.actor_spawn_zones[info.spawn_zone_index].xz_bounds(&map_geometry);
        if xz_distance_from_rect(pos, zone_bounds) > kind_server_config.max_wander_distance {
            if !info.is_returning_to_spawn {
                start_chase_reacquire_cooldown_if_chasing(info, kind_server_config.chase_reacquire_cooldown);
                set_return_path_to_spawn_zone(
                    info,
                    pos,
                    &map_config.actor_spawn_zones[info.spawn_zone_index],
                    &nav_graph,
                    zone_bounds,
                );
            }
            info.go_to_position_is_chase = false;
            continue;
        }

        if !chase_reacquire_blocked
            && (info.go_to_position.is_none() || info.go_to_position_is_chase)
            && let Some(target_pos) = visible_player_position(
                pos,
                actor_config.eye_height(),
                kind_server_config.vision_range,
                &players,
                &player_query,
                &collision_world,
                &gameplay_config,
            )
        {
            info.return_path.clear();
            info.is_returning_to_spawn = false;
            info.go_to_position = Some(target_pos);
            info.go_to_position_is_chase = true;
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

fn tick_chase_reacquire_timer(info: &mut crate::resources::ActorInfo, delta: f32) -> bool {
    if info.chase_reacquire_timer <= 0.0 {
        return false;
    }
    info.chase_reacquire_timer = (info.chase_reacquire_timer - delta).max(0.0);
    info.chase_reacquire_timer > 0.0
}

fn start_chase_reacquire_cooldown_if_chasing(info: &mut crate::resources::ActorInfo, chase_reacquire_cooldown: f32) {
    if info.go_to_position_is_chase {
        info.chase_reacquire_timer = chase_reacquire_cooldown;
    }
}

fn set_return_path_to_spawn_zone(
    info: &mut crate::resources::ActorInfo,
    pos: &Position,
    zone: &crate::resources::ActorSpawnZone,
    nav_graph: &NavGraph,
    zone_bounds: (f32, f32, f32, f32),
) {
    info.return_path = nav_graph.path_to_spawn_zone(pos, zone).unwrap_or_default();
    info.go_to_position = info
        .return_path
        .pop_front()
        .or_else(|| Some(closest_point_in_rect(pos, zone_bounds)));
    info.is_returning_to_spawn = true;
}

#[cfg(test)]
mod tests {
    use bevy::prelude::Entity;
    use common::protocol::ActorMoveIntent;

    use super::*;
    use crate::resources::{ActorAvoidanceState, ActorInfo};

    fn actor_info(chase_reacquire_timer: f32) -> ActorInfo {
        ActorInfo {
            entity: Entity::from_bits(1),
            spawn_zone_index: 0,
            spawn_kind: "mine_1".into(),
            direction_timer: 0.0,
            patrol_intent: ActorMoveIntent::Idle,
            go_to_position: None,
            go_to_position_is_chase: false,
            is_returning_to_spawn: false,
            return_path: Default::default(),
            chase_reacquire_timer,
            avoidance_state: ActorAvoidanceState::None,
            last_broadcast_move_intent: ActorMoveIntent::Idle,
            move_intent_send_timer: 0.0,
        }
    }

    #[test]
    fn chase_reacquire_timer_blocks_until_elapsed() {
        let mut info = actor_info(1.0);

        assert!(tick_chase_reacquire_timer(&mut info, 0.25));
        assert_eq!(info.chase_reacquire_timer, 0.75);
        assert!(!tick_chase_reacquire_timer(&mut info, 0.75));
        assert_eq!(info.chase_reacquire_timer, 0.0);
    }

    #[test]
    fn interrupted_chase_starts_reacquire_cooldown() {
        let mut info = actor_info(0.0);
        info.go_to_position = Some(Position { x: 1.0, y: 0.0, z: 1.0 });
        info.go_to_position_is_chase = true;

        start_chase_reacquire_cooldown_if_chasing(&mut info, 5.0);

        assert_eq!(info.chase_reacquire_timer, 5.0);
    }

    #[test]
    fn non_chase_go_to_does_not_start_reacquire_cooldown() {
        let mut info = actor_info(0.0);
        info.go_to_position = Some(Position { x: 1.0, y: 0.0, z: 1.0 });

        start_chase_reacquire_cooldown_if_chasing(&mut info, 5.0);

        assert_eq!(info.chase_reacquire_timer, 0.0);
    }

    #[test]
    fn setting_return_path_marks_actor_as_returning() {
        let mut cells = crate::resources::CellGrid::new(1, 1);
        cells.rows[0][0].has_floor = true;
        let map_config = MapConfig {
            levels: vec![crate::resources::LevelGrid {
                cells,
                edges: crate::resources::EdgeGrid::new(1, 1),
            }],
            actor_spawn_zones: Vec::new(),
            player_spawn_zones: Vec::new(),
        };
        let nav_graph = NavGraph::new(map_config, MapGeometry::new(1, 1));
        let zone = crate::resources::ActorSpawnZone {
            level: 0,
            cols: [0, 1],
            rows: [0, 1],
            kind: "mine_1".into(),
            count: 1,
        };
        let mut info = actor_info(0.0);

        set_return_path_to_spawn_zone(
            &mut info,
            &Position { x: 0.0, y: 0.0, z: 0.0 },
            &zone,
            &nav_graph,
            (-2.0, -2.0, 2.0, 2.0),
        );

        assert!(info.is_returning_to_spawn);
        assert!(info.go_to_position.is_some());
    }
}
