use bevy_math::Vec3;

use crate::{
    config::CharacterPhysicsConfig,
    protocol::{HomingTarget, Position},
};

use super::{CollisionWorld, projectiles::ball_character_hit};

// Thin ray for the world-occlusion test, so walls block locks at their real
// silhouette regardless of how generous the aim assist is.
const LOCK_RAY_RADIUS: f32 = 0.05;

// Pick the lock-on target under the aim ray: nearest candidate the ray
// passes within `assist_radius` of, capped at `max_distance`; world geometry
// occludes (barriers don't block sight, matching `line_of_sight_clear`
// semantics — the missile flies around them). Shared by the client's
// crosshair lock and the server's fire re-validation.
#[must_use]
pub fn acquire_lock(
    collision_world: &CollisionWorld,
    origin: Vec3,
    aim_dir: Vec3,
    max_distance: f32,
    assist_radius: f32,
    candidates: impl Iterator<Item = (HomingTarget, Position, f32, CharacterPhysicsConfig)>,
) -> Option<HomingTarget> {
    let translation = aim_dir.normalize_or_zero() * max_distance;
    if translation.length_squared() <= f32::EPSILON {
        return None;
    }
    let world_t = collision_world
        .cast_moving_ball(origin, translation, LOCK_RAY_RADIUS)
        .map_or(1.0, |hit| hit.t);
    let origin_pos = Position::from(origin);

    let mut best: Option<(f32, HomingTarget)> = None;
    for (target, pos, face_dir, physics) in candidates {
        let Some(hit) = ball_character_hit(&origin_pos, translation, assist_radius, 1.0, &pos, face_dir, physics)
        else {
            continue;
        };
        if hit.time_of_impact >= world_t {
            continue;
        }
        if best.is_none_or(|(best_t, _)| hit.time_of_impact < best_t) {
            best = Some((hit.time_of_impact, target));
        }
    }
    best.map(|(_, target)| target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{
            CharacterColliderAnchor, CharacterColliderConfig, CharacterPhysicsConfig, CharacterSupportProbeConfig,
        },
        constants::WALL_THICKNESS,
        protocol::{ActorId, BarrierKindTable, MapLayout, PlayerId, Wall},
    };

    fn physics() -> CharacterPhysicsConfig {
        CharacterPhysicsConfig {
            collider: CharacterColliderConfig {
                width: 1.0,
                height: 1.3,
                depth: 0.6,
                y_offset: 0.5,
                y_offset_anchor: CharacterColliderAnchor::Bottom,
            },
            support_probe: CharacterSupportProbeConfig { width: 0.2, depth: 0.2 },
        }
    }

    fn empty_world() -> CollisionWorld {
        CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default())
    }

    fn candidate(target: HomingTarget, z: f32) -> (HomingTarget, Position, f32, CharacterPhysicsConfig) {
        (target, Position { x: 0.0, y: 0.0, z }, 0.0, physics())
    }

    #[test]
    fn acquire_lock_picks_nearest_candidate_on_ray() {
        let near = HomingTarget::Player(PlayerId(1));
        let far = HomingTarget::Actor(ActorId(2));
        let locked = acquire_lock(
            &empty_world(),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::Z,
            60.0,
            0.05,
            [candidate(far, 20.0), candidate(near, 5.0)].into_iter(),
        );
        assert_eq!(locked, Some(near));
    }

    #[test]
    fn acquire_lock_misses_candidate_off_ray() {
        let target = HomingTarget::Player(PlayerId(1));
        let locked = acquire_lock(
            &empty_world(),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::Z,
            60.0,
            0.05,
            [(target, Position { x: 8.0, y: 0.0, z: 5.0 }, 0.0, physics())].into_iter(),
        );
        assert_eq!(locked, None);
    }

    #[test]
    fn acquire_lock_rejects_candidate_beyond_range() {
        let target = HomingTarget::Player(PlayerId(1));
        let locked = acquire_lock(
            &empty_world(),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::Z,
            10.0,
            0.05,
            [candidate(target, 20.0)].into_iter(),
        );
        assert_eq!(locked, None);
    }

    #[test]
    fn acquire_lock_rejects_candidate_behind_wall() {
        let layout = MapLayout {
            walls: vec![Wall {
                x1: -4.0,
                z1: 5.0,
                x2: 4.0,
                z2: 5.0,
                width: WALL_THICKNESS,
                level: 0,
            }],
            ..Default::default()
        };
        let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
        let target = HomingTarget::Player(PlayerId(1));
        let locked = acquire_lock(
            &world,
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::Z,
            60.0,
            0.05,
            [candidate(target, 10.0)].into_iter(),
        );
        assert_eq!(locked, None);
    }

    #[test]
    fn acquire_lock_assist_radius_forgives_near_misses() {
        let target = HomingTarget::Player(PlayerId(1));
        // ~1 m off the aim line (collider half-width 0.5 leaves ~0.5 m gap).
        let off_axis = (target, Position { x: 1.0, y: 0.0, z: 8.0 }, 0.0, physics());
        let aim = Vec3::new(0.0, 1.0, 0.0);

        let strict = acquire_lock(&empty_world(), aim, Vec3::Z, 60.0, 0.05, [off_axis].into_iter());
        assert_eq!(strict, None, "thin ray misses the off-axis target");

        let assisted = acquire_lock(&empty_world(), aim, Vec3::Z, 60.0, 1.2, [off_axis].into_iter());
        assert_eq!(assisted, Some(target), "assist radius bridges the gap");
    }
}
