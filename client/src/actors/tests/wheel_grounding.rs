use bevy::prelude::*;
use common::{
    config::{CharacterPhysicsConfig, HitboxConfig, MovementColliderConfig},
    physics::CollisionWorld,
    protocol::{ActorMarker, CarrierId, Floor, MapLayout, Position, Ramp},
};

use super::wheel_grounding::{WheelGrounding, ground_pose, wheel_grounding_system};
use crate::config::WheelModelDef;

fn wheels() -> WheelModelDef {
    WheelModelDef {
        radius: 0.215,
        track: 0.87,
        wheelbase: 0.58,
        idle_animation: 0,
        drive_animation: 1,
        drive_cycle_secs: 1.0,
    }
}

fn physics() -> CharacterPhysicsConfig {
    CharacterPhysicsConfig {
        movement_collider: MovementColliderConfig {
            diameter: 1.05,
            height: 1.1,
        },
        hitbox: HitboxConfig {
            width: 1.1,
            height: 0.88,
            depth: 1.02,
            bottom_offset: 0.0,
        },
    }
}

fn grounding() -> WheelGrounding {
    WheelGrounding {
        owner: Entity::PLACEHOLDER,
        physics: physics(),
        wheels: wheels(),
        rest: Transform::IDENTITY,
    }
}

fn ramp_world() -> CollisionWorld {
    CollisionWorld::from_map_layout(&MapLayout {
        ramps: vec![Ramp {
            x1: -4.0,
            x2: 4.0,
            z1: 0.0,
            z2: 12.0,
            y1: 0.0,
            y2: 6.0,
            carrier: CarrierId::WORLD,
        }],
        floors: vec![Floor {
            x1: -4.0,
            x2: 4.0,
            z1: -4.0,
            z2: 0.0,
            y: 0.0,
            thickness: 0.2,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..default()
    })
}

#[test]
fn tyres_rest_on_ramp_when_driving_up_down_or_across_it() {
    let world = ramp_world();
    let normal = Vec3::new(0.0, 1.0, -0.5).normalize();
    let position = Position {
        x: 0.0,
        y: 3.0 + physics().movement_collider.radius() * (normal.y.recip() - 1.0),
        z: 6.0,
    };
    for angle in [0.0, 1.0, std::f32::consts::FRAC_PI_2, std::f32::consts::PI] {
        let yaw = Quat::from_rotation_y(angle);
        let (rotation, offset) = ground_pose(&world, &grounding(), position, position.into(), yaw)
            .expect("ramp pose missing for a supported scuttler");
        assert!((rotation * Vec3::Y).abs_diff_eq(normal, 1e-4));
        assert!(
            (rotation * Vec3::Z)
                .xz()
                .normalize()
                .abs_diff_eq((yaw * Vec3::Z).xz(), 1e-4)
        );
        for x in [-0.435, 0.435] {
            for z in [-0.29, 0.29] {
                let centre = Vec3::from(position) + Vec3::Y * offset + rotation * Vec3::new(x, wheels().radius, z);
                assert!(
                    (normal.dot(centre) - wheels().radius).abs() < 1e-4,
                    "tyre does not touch the ramp"
                );
            }
        }
    }
}

#[test]
fn flat_floor_is_level_and_airborne_scuttlers_are_not_pulled_down() {
    let world = ramp_world();
    let yaw = Quat::from_rotation_y(0.7);
    let position = Position {
        x: 0.0,
        y: 0.0,
        z: -2.0,
    };
    let (rotation, offset) =
        ground_pose(&world, &grounding(), position, position.into(), yaw).expect("flat ground pose missing");
    assert!(rotation.abs_diff_eq(yaw, 1e-5));
    assert!(offset.abs() < 1e-5);
    let airborne = Position { y: 1.0, ..position };
    assert!(ground_pose(&world, &grounding(), airborne, airborne.into(), yaw).is_none());
}

#[test]
fn visual_tilt_preserves_the_actor_and_its_collider_frame() {
    let mut app = App::new();
    app.insert_resource(ramp_world());
    app.add_systems(Update, wheel_grounding_system);
    let position = Position {
        x: 0.0,
        y: 3.062,
        z: 6.0,
    };
    let parent = Transform::from_translation(position.into()).with_rotation(Quat::from_rotation_y(0.7));
    let owner = app.world_mut().spawn((ActorMarker, position, parent)).id();
    let model = app
        .world_mut()
        .spawn((WheelGrounding { owner, ..grounding() }, Transform::IDENTITY))
        .id();
    app.update();
    assert_eq!(
        *app.world().get::<Transform>(owner).expect("actor transform missing"),
        parent
    );
    let model = app.world().get::<Transform>(model).expect("scuttler transform missing");
    assert!((parent.rotation * model.rotation * Vec3::Y).abs_diff_eq(Vec3::new(0.0, 1.0, -0.5).normalize(), 1e-4));
}

#[test]
fn wheel_footprint_blends_the_tilt_at_the_foot_of_a_ramp() {
    let world = ramp_world();
    let normal = Vec3::new(0.0, 1.0, -0.5).normalize();
    let rise = physics().movement_collider.radius() * (normal.y.recip() - 1.0);
    let mut previous_angle = 0.0;
    for step in -40..=40 {
        let z = step as f32 * 0.01;
        let position = Position {
            x: 0.0,
            y: (z * 0.5 + rise).max(0.0),
            z,
        };
        let (rotation, _) = ground_pose(&world, &grounding(), position, position.into(), Quat::IDENTITY)
            .expect("ground pose missing at the ramp transition");
        let angle = (rotation * Vec3::Y).angle_between(Vec3::Y);
        assert!(angle + 1e-4 >= previous_angle);
        assert!(angle - previous_angle < 0.012, "tilt snapped at the ramp transition");
        previous_angle = angle;
    }
    assert!((previous_angle - normal.angle_between(Vec3::Y)).abs() < 1e-4);
}
