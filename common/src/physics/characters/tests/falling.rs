use crate::{
    config::{CharacterPhysicsConfig, FallDamageConfig, HitboxConfig, MovementColliderConfig},
    map::Carriers,
    physics::{
        CharacterEnvironment, CharacterStep, CharacterSupport, CollisionWorld, LadderMode, fall_damage_for_distance,
        fall_distance_for_speed, landing_damage, step_character_movement,
    },
    protocol::{CarrierId, Floor, MapLayout, Position},
};
use bevy_math::Vec3;

const TEST_GRAVITY: f32 = 25.0;

fn thresholds(safe_distance: f32, lethal_distance: f32) -> FallDamageConfig {
    FallDamageConfig {
        safe_distance,
        lethal_distance,
    }
}

#[test]
fn simulated_tall_fall_reaches_lethal_damage_without_low_gravity() {
    let world = CollisionWorld::from_map_layout(&MapLayout {
        floors: vec![Floor {
            x1: -4.0,
            x2: 4.0,
            z1: -4.0,
            z2: 4.0,
            y: 0.0,
            thickness: 0.4,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    });
    let carriers = Carriers::default();
    for gravity in [24.0, 13.2] {
        let environment = CharacterEnvironment {
            collision_world: &world,
            carriers: &carriers,
            gravity,
            physics: CharacterPhysicsConfig {
                movement_collider: MovementColliderConfig {
                    diameter: 0.6,
                    height: 1.8,
                },
                hitbox: HitboxConfig {
                    width: 0.6,
                    height: 1.8,
                    depth: 0.6,
                    bottom_offset: 0.0,
                },
            },
            passable_fields: &[],
            ladder_mode: LadderMode::Automatic,
            portals: None,
        };
        let mut pos = Position {
            y: 21.6,
            ..Default::default()
        };
        let mut vertical_velocity = 0.0;
        let mut impact = None;
        for _ in 0..300 {
            let result = step_character_movement(
                CharacterStep {
                    start: pos,
                    vertical_velocity,
                    control_velocity: Vec3::ZERO,
                    external_displacement: Vec3::ZERO,
                    delta: 1.0 / 30.0,
                },
                &environment,
            );
            pos = result.position;
            vertical_velocity = result.vertical_velocity;
            if result.support == CharacterSupport::Ground {
                impact = Some(result.impact_speed);
                break;
            }
        }
        let distance = fall_distance_for_speed(impact.expect("the fall never landed"), 24.0);
        let damage = fall_damage_for_distance(distance, &thresholds(8.0, 15.0), 100.0);
        if gravity == 24.0 {
            assert_eq!(
                damage, 100.0,
                "normal-gravity fall was not lethal: effective drop {distance}"
            );
        } else {
            assert!(
                (50.0..60.0).contains(&damage),
                "low gravity lost its protection: damage {damage}"
            );
        }
    }
}

#[test]
fn fall_damage_zero_at_safe_distance() {
    assert_eq!(fall_damage_for_distance(4.0, &thresholds(4.0, 12.0), 100.0), 0.0);
    assert_eq!(fall_damage_for_distance(3.0, &thresholds(4.0, 12.0), 100.0), 0.0);
}

#[test]
fn fall_damage_lethal_at_lethal_distance() {
    assert_eq!(fall_damage_for_distance(12.0, &thresholds(4.0, 12.0), 100.0), 100.0);
}

#[test]
fn fall_damage_lerps_midpoint() {
    // (8 - 4) / (12 - 4) = 0.5 → 50 dmg
    assert_eq!(fall_damage_for_distance(8.0, &thresholds(4.0, 12.0), 100.0), 50.0);
}

#[test]
fn fall_damage_saturates_past_lethal() {
    assert_eq!(fall_damage_for_distance(100.0, &thresholds(4.0, 12.0), 100.0), 100.0);
}

#[test]
fn landing_damage_is_none_below_the_emit_threshold() {
    let fall = thresholds(4.0, 12.0);
    let speed = (2.0_f32 * TEST_GRAVITY * 4.04).sqrt();
    assert_eq!(landing_damage(speed, TEST_GRAVITY, &fall, 100.0), None);
    let damage = landing_damage(speed, TEST_GRAVITY, &fall, 1000.0).expect("a 5 hp landing is not soft");
    assert!((damage - 5.0).abs() < 0.05, "{damage}");
    assert_eq!(landing_damage(0.0, TEST_GRAVITY, &fall, 100.0), None);
}

#[test]
fn impact_energy_determines_the_equivalent_drop() {
    assert_eq!(fall_distance_for_speed(0.0, TEST_GRAVITY), 0.0);
    assert_eq!(fall_distance_for_speed(10.0, TEST_GRAVITY), 2.0);
    assert_eq!(fall_distance_for_speed(20.0, TEST_GRAVITY), 8.0);
    assert_eq!(fall_distance_for_speed(25.0, TEST_GRAVITY), 12.5);
}
