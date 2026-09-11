use super::*;
use crate::test_fixtures::{WALL_HEIGHT, WALL_THICKNESS};
use common::{
    config::{CharacterPhysicsConfig, HitboxConfig, MovementColliderConfig},
    protocol::{ActorId, CarrierId, MapLayout, PlayerId, Wall},
};

fn physics() -> CharacterPhysicsConfig {
    CharacterPhysicsConfig {
        hitbox: HitboxConfig {
            width: 1.0,
            height: 1.3,
            depth: 0.6,
            bottom_offset: 0.5,
        },
        movement_collider: MovementColliderConfig {
            diameter: 0.6,
            height: 1.8,
        },
    }
}

fn empty_world() -> CollisionWorld {
    CollisionWorld::from_map_layout(&MapLayout::default())
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
            y: 0.0,
            height: WALL_HEIGHT,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout);
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
