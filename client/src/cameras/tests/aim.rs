use super::*;
use bevy::camera::CameraProjection;
use common::{
    config::{HitboxConfig, MovementColliderConfig},
    protocol::{BarrierKindTable, CarrierId, MapLayout, Wall},
};

#[test]
fn raised_crosshair_ray_matches_projection_across_fovs_and_camera_rotations() {
    for fov in [60.0_f32, 90.0, 110.0] {
        let projection = PerspectiveProjection {
            fov: fov.to_radians(),
            aspect_ratio: 16.0 / 9.0,
            ..default()
        };
        let camera = Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, 0.4, -0.3, 0.1));
        for offset in [0.0, 0.28, 0.4] {
            let ray = crosshair_direction(&camera, &projection, offset);
            let local_target = camera.rotation.inverse() * ray * 20.0;
            let ndc = projection.get_clip_from_view().project_point3(local_target);
            assert!(ndc.x.abs() < 1e-5);
            assert!((ndc.y - offset * 2.0).abs() < 1e-5);
        }
    }
}

#[test]
fn raised_crosshair_and_shot_reach_the_same_wall_point() {
    let world = CollisionWorld::from_map_layout(
        &MapLayout {
            walls: vec![Wall {
                x1: -10.0,
                z1: -20.0,
                x2: 10.0,
                z2: -20.0,
                width: 0.2,
                y: 0.0,
                height: 30.0,
                level: 0,
                carrier: CarrierId::WORLD,
            }],
            ..default()
        },
        &BarrierKindTable::default(),
    );
    let eye = Vec3::new(0.0, 1.7, 0.0);
    let camera = Transform::from_xyz(0.5, 1.4, 4.0);
    let ray = crosshair_direction(&camera, &PerspectiveProjection::default(), 0.15);
    let target = world
        .attack_surface_along_ray(camera.translation, ray, 100.0, &[])
        .expect("crosshair ray missed wall");
    let direction = third_person_aim(&world, camera.translation, ray, eye, &[], std::iter::empty());
    let shot = world
        .attack_surface_along_ray(eye, direction, 100.0, &[])
        .expect("shot missed wall");
    assert!(shot.point.distance(target.point) < 1e-4);
    assert!(shot.point.y > eye.y + 1.0);
}

#[test]
fn shoulder_camera_converges_on_near_character() {
    let world = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());
    let physics = CharacterPhysicsConfig {
        movement_collider: MovementColliderConfig {
            diameter: 0.6,
            height: 1.8,
        },
        hitbox: HitboxConfig {
            width: 0.6,
            depth: 0.6,
            height: 1.8,
            bottom_offset: 0.0,
        },
    };
    let eye = Vec3::new(0.0, 1.4, 0.0);
    let direction = third_person_aim(
        &world,
        Vec3::new(0.45, 1.4, 4.0),
        Vec3::NEG_Z,
        eye,
        &[],
        [(Position::from(Vec3::new(0.45, 0.0, -3.0)), 0.0, physics)].into_iter(),
    );
    assert!(direction.x > 0.1 && direction.z < -0.9);
    assert!(
        ball_character_hit(
            &eye.into(),
            direction * 10.0,
            0.01,
            1.0,
            &Position::from(Vec3::new(0.45, 0.0, -3.0)),
            0.0,
            physics
        )
        .is_some()
    );
}

#[test]
fn shoulder_visibility_does_not_bypass_muzzle_cover() {
    let world = CollisionWorld::from_map_layout(
        &MapLayout {
            walls: vec![Wall {
                x1: -1.0,
                z1: -1.0,
                x2: 0.3,
                z2: -1.0,
                width: 0.2,
                y: 0.0,
                height: 3.0,
                level: 0,
                carrier: CarrierId::WORLD,
            }],
            ..default()
        },
        &BarrierKindTable::default(),
    );
    let eye = Vec3::new(0.0, 1.4, 0.0);
    let camera = Vec3::new(0.45, 1.4, 4.0);
    assert!(world.world_surface_along_ray(camera, Vec3::NEG_Z, 20.0).is_none());
    let direction = third_person_aim(&world, camera, Vec3::NEG_Z, eye, &[], std::iter::empty());
    assert!(world.attack_surface_along_ray(eye, direction, 20.0, &[]).is_some());
    assert!(!world.projectile_path_clear(eye, direction * 2.0, 0.05, &[]));
}
