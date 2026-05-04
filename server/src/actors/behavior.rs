use bevy::prelude::*;
use rand::{RngExt, rng, rngs::ThreadRng};

use crate::{
    config::{ActorKindServerConfig, ServerGameplayConfig},
    resources::{ActorMap, MapConfig, PlayerMap},
};
use common::{
    config::GameplayConfig,
    physics::CollisionWorld,
    protocol::{ActorId, ActorMarker, ActorMoveIntent, PlayerId, PlayerMarker, Position},
};

pub fn actor_behavior_system(
    time: Res<Time>,
    players: Res<PlayerMap>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    map_config: Res<MapConfig>,
    mut actors: ResMut<ActorMap>,
    player_query: Query<(&PlayerId, &Position), With<PlayerMarker>>,
    query: Query<(&ActorId, &Position), (With<ActorMarker>, Without<PlayerMarker>)>,
) {
    let delta = time.delta_secs();
    let mut rng = rng();

    for (id, pos) in &query {
        let Some(info) = actors.0.get_mut(id) else {
            continue;
        };
        // Per-kind config: cross-validated against the map at startup, so unwraps are safe.
        let actor_config = gameplay_config
            .actor(&info.spawn_kind)
            .expect("actor kind validated at startup");
        let kind_server_config = server_gameplay_config
            .actor(&info.spawn_kind)
            .expect("actor kind validated at startup");

        // Wander limit: if the actor has strayed past `max_wander_distance`
        // from the nearest edge of its spawn zone, override everything else
        // and walk it back. This naturally cancels chases that carry the
        // actor too far — the next tick rewrites `go_to_position` from the
        // player's location to a point inside the zone.
        let zone_bounds = map_config.actor_spawn_zones[info.spawn_zone_index].xz_bounds();
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

// Euclidean xz-distance from `pos` to the nearest edge of the
// `(min_x, min_z, max_x, max_z)` rectangle. Inside the rectangle returns 0.
fn xz_distance_from_rect(pos: &Position, bounds: (f32, f32, f32, f32)) -> f32 {
    let (min_x, min_z, max_x, max_z) = bounds;
    let dx = (min_x - pos.x).max(0.0).max(pos.x - max_x);
    let dz = (min_z - pos.z).max(0.0).max(pos.z - max_z);
    dx.hypot(dz)
}

// Closest point on the rectangle (in xz) to `pos`. y is preserved so the
// actor doesn't try to climb to a different floor.
fn closest_point_in_rect(pos: &Position, bounds: (f32, f32, f32, f32)) -> Position {
    let (min_x, min_z, max_x, max_z) = bounds;
    Position {
        x: pos.x.clamp(min_x, max_x),
        y: pos.y,
        z: pos.z.clamp(min_z, max_z),
    }
}

pub(crate) fn random_patrol_intent(rng: &mut ThreadRng, patrol_speed: f32, idle_probability: f32) -> ActorMoveIntent {
    if rng.random_range(0.0..1.0) < idle_probability {
        ActorMoveIntent::Idle
    } else {
        random_patrol_move_intent(rng, patrol_speed)
    }
}

pub(crate) fn random_patrol_move_intent(rng: &mut ThreadRng, patrol_speed: f32) -> ActorMoveIntent {
    ActorMoveIntent::Moving {
        direction: rng.random_range(0.0..std::f32::consts::TAU),
        speed: patrol_speed,
    }
}

fn visible_player_position(
    actor_pos: &Position,
    actor_eye_height: f32,
    vision_range: f32,
    players: &PlayerMap,
    player_query: &Query<(&PlayerId, &Position), With<PlayerMarker>>,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
) -> Option<Position> {
    let actor_sight_origin = Vec3::new(actor_pos.x, actor_pos.y + actor_eye_height, actor_pos.z);
    let player_physics = gameplay_config.player.physics();

    player_query
        .iter()
        .filter(|(id, _)| players.0.get(id).is_some_and(|info| info.logged_in))
        .filter(|(_, pos)| horizontal_distance_sq(actor_pos, pos) <= vision_range * vision_range)
        .filter(|(_, pos)| {
            let player_collider_center = Vec3::new(pos.x, player_physics.collider_center_y(pos.y), pos.z);
            collision_world.line_of_sight_clear(actor_sight_origin, player_collider_center)
        })
        .min_by(|(_, a), (_, b)| horizontal_distance_sq(actor_pos, a).total_cmp(&horizontal_distance_sq(actor_pos, b)))
        .map(|(_, pos)| *pos)
}

fn horizontal_distance_sq(a: &Position, b: &Position) -> f32 {
    let dx = a.x - b.x;
    let dz = a.z - b.z;
    dx.mul_add(dx, dz * dz)
}

fn random_direction_time(rng: &mut ThreadRng, kind_server_config: &ActorKindServerConfig) -> f32 {
    rng.random_range(kind_server_config.min_direction_time..=kind_server_config.max_direction_time)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(x: f32, z: f32) -> Position {
        Position { x, y: 0.0, z }
    }

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-4,
            "expected {actual} to be near {expected}"
        );
    }

    #[test]
    fn xz_distance_from_rect_zero_inside() {
        let bounds = (-2.0, -2.0, 2.0, 2.0);
        assert_eq!(xz_distance_from_rect(&pos(0.0, 0.0), bounds), 0.0);
        assert_eq!(xz_distance_from_rect(&pos(-2.0, -2.0), bounds), 0.0);
        assert_eq!(xz_distance_from_rect(&pos(2.0, 2.0), bounds), 0.0);
        assert_eq!(xz_distance_from_rect(&pos(1.5, -1.5), bounds), 0.0);
    }

    #[test]
    fn xz_distance_from_rect_axis_aligned_outside() {
        let bounds = (-2.0, -2.0, 2.0, 2.0);
        assert_near(xz_distance_from_rect(&pos(5.0, 0.0), bounds), 3.0); // east
        assert_near(xz_distance_from_rect(&pos(-5.0, 0.0), bounds), 3.0); // west
        assert_near(xz_distance_from_rect(&pos(0.0, 5.0), bounds), 3.0); // south
        assert_near(xz_distance_from_rect(&pos(0.0, -5.0), bounds), 3.0); // north
    }

    #[test]
    fn xz_distance_from_rect_corner() {
        let bounds = (-2.0, -2.0, 2.0, 2.0);
        // Point at (5, 6) — nearest corner is (2, 2). dx=3, dz=4 → 5.
        assert_near(xz_distance_from_rect(&pos(5.0, 6.0), bounds), 5.0);
    }

    #[test]
    fn closest_point_in_rect_inside_returns_unchanged() {
        let bounds = (-2.0, -2.0, 2.0, 2.0);
        let p = Position {
            x: 1.0,
            y: 7.5,
            z: -0.5,
        };
        let clamped = closest_point_in_rect(&p, bounds);
        assert_eq!(
            clamped,
            Position {
                x: 1.0,
                y: 7.5,
                z: -0.5
            }
        );
    }

    #[test]
    fn closest_point_in_rect_clamps_to_edge() {
        let bounds = (-2.0, -2.0, 2.0, 2.0);
        let p = Position { x: 5.0, y: 7.5, z: 0.0 };
        let clamped = closest_point_in_rect(&p, bounds);
        assert_eq!(clamped, Position { x: 2.0, y: 7.5, z: 0.0 });
    }

    #[test]
    fn closest_point_in_rect_clamps_to_corner() {
        let bounds = (-2.0, -2.0, 2.0, 2.0);
        let p = Position {
            x: 5.0,
            y: 7.5,
            z: -10.0,
        };
        let clamped = closest_point_in_rect(&p, bounds);
        assert_eq!(
            clamped,
            Position {
                x: 2.0,
                y: 7.5,
                z: -2.0
            }
        );
    }

    #[test]
    fn closest_point_in_rect_preserves_y() {
        let bounds = (-2.0, -2.0, 2.0, 2.0);
        let p = Position {
            x: 100.0,
            y: 42.0,
            z: 100.0,
        };
        let clamped = closest_point_in_rect(&p, bounds);
        assert_eq!(clamped.y, 42.0);
    }
}
