use super::super::fixtures::{wall, world};
use super::*;
use common::{
    config::{HitboxConfig, MovementColliderConfig},
    protocol::{ActorId, MapLayout, PlayerId},
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

fn candidate(target: HomingTarget, x: f32, z: f32) -> (HomingTarget, Position, f32, CharacterPhysicsConfig) {
    (target, Position { x, y: 0.0, z }, 0.0, physics())
}

// The lock from just above the origin looking along +Z.
fn lock(
    world: &CollisionWorld,
    max_distance: f32,
    assist_radius: f32,
    candidates: &[(HomingTarget, Position, f32, CharacterPhysicsConfig)],
) -> Option<HomingTarget> {
    acquire_lock(
        world,
        &[],
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::Z,
        max_distance,
        assist_radius,
        candidates.iter().copied(),
    )
}

#[test]
fn acquire_lock_picks_nearest_candidate_on_ray() {
    let near = HomingTarget::Player(PlayerId(1));
    let far = HomingTarget::Actor(ActorId(2));
    let open = world(&MapLayout::default());
    assert_eq!(
        lock(
            &open,
            60.0,
            0.05,
            &[candidate(far, 0.0, 20.0), candidate(near, 0.0, 5.0)]
        ),
        Some(near)
    );
}

#[test]
fn acquire_lock_stops_at_its_range_and_at_a_wall() {
    let target = HomingTarget::Player(PlayerId(1));
    let open = world(&MapLayout::default());
    assert_eq!(lock(&open, 10.0, 0.05, &[candidate(target, 0.0, 20.0)]), None);
    let walled = world(&MapLayout {
        walls: vec![wall(-4.0, 5.0, 4.0, 5.0)],
        ..Default::default()
    });
    assert_eq!(lock(&walled, 60.0, 0.05, &[candidate(target, 0.0, 10.0)]), None);
}

#[test]
fn acquire_lock_assist_radius_forgives_near_misses() {
    let target = HomingTarget::Player(PlayerId(1));
    let open = world(&MapLayout::default());
    // ~1 m off the aim line (collider half-width 0.5 leaves ~0.5 m gap).
    let off_axis = [candidate(target, 1.0, 8.0)];
    assert_eq!(
        lock(&open, 60.0, 0.05, &off_axis),
        None,
        "thin ray misses the off-axis target"
    );
    assert_eq!(
        lock(&open, 60.0, 1.2, &off_axis),
        Some(target),
        "assist radius bridges the gap"
    );
}
