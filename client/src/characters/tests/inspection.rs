use std::time::Duration;

use common::{
    config::{HitboxConfig, MovementColliderConfig},
    protocol::PlayerMarker,
};

use super::*;
use crate::{
    characters::PreviousTickPosition,
    players::{LocalPlayerMarker, players_transform_sync_system},
};

#[test]
fn bounds_keep_world_dimensions_and_physics_facing() {
    let parent = Transform::from_xyz(3.0, 1.0, 2.0)
        .with_rotation(Quat::from_rotation_y(0.8))
        .with_scale(Vec3::splat(1.5));
    let center = Vec3::new(3.1, 1.9, 2.2);
    for rotation in [Quat::IDENTITY, Quat::from_rotation_y(-1.3)] {
        let child = bounds_transform(&parent, center, rotation);
        let world = parent.mul_transform(child);
        assert!(world.translation.abs_diff_eq(center, 1e-5));
        assert!(world.rotation.abs_diff_eq(rotation, 1e-5));
        assert!(world.scale.abs_diff_eq(Vec3::ONE, 1e-5));
    }
}

#[test]
fn bounds_interpolate_between_ticks_without_animation_or_hit_shake() {
    let physics = CharacterPhysicsConfig {
        movement_collider: MovementColliderConfig {
            diameter: 0.6,
            height: 1.8,
        },
        hitbox: HitboxConfig {
            width: 0.7,
            height: 1.6,
            depth: 0.5,
            bottom_offset: 0.1,
        },
    };
    for mode in [BoundsMode::Grounding, BoundsMode::Hitbox] {
        let mut app = App::new();
        app.insert_resource(mode).add_systems(
            Update,
            (players_transform_sync_system, character_bounds_sync_system).chain(),
        );
        let player = app
            .world_mut()
            .spawn((
                PlayerMarker,
                LocalPlayerMarker,
                Position { x: 4.0, y: 0.0, z: 0.0 },
                PreviousTickPosition(Position { x: 3.0, y: 0.0, z: 0.0 }),
                FaceYaw(-1.3),
                Transform::default()
                    .with_rotation(Quat::from_rotation_y(0.8))
                    .with_scale(Vec3::splat(1.5)),
                CuboidShake {
                    timer: Timer::from_seconds(1.0, TimerMode::Once),
                    intensity: 1.0,
                    dir_x: 1.0,
                    dir_z: 1.0,
                    offset_x: 0.0,
                    offset_z: 0.0,
                },
            ))
            .id();
        let model = app.world_mut().spawn((ChildOf(player), Transform::default())).id();
        let root = app
            .world_mut()
            .spawn((ChildOf(player), CharacterBounds { physics }, Transform::default()))
            .id();
        let shape = app
            .world_mut()
            .spawn((
                ChildOf(root),
                BoundsShape(mode),
                Transform::default(),
                Visibility::Hidden,
            ))
            .id();
        for alpha in [0.25, 0.5, 0.75] {
            let mut fixed = Time::<Fixed>::from_hz(1.0);
            fixed.accumulate_overstep(Duration::from_secs_f32(alpha));
            app.insert_resource(fixed);
            app.world_mut()
                .get_mut::<CuboidShake>(player)
                .expect("player shake missing")
                .offset_x = alpha * 0.2;
            app.world_mut()
                .get_mut::<Transform>(model)
                .expect("model transform missing")
                .translation
                .y = alpha.sin();
            app.update();
            let parent = app.world().get::<Transform>(player).expect("player transform missing");
            let child = app.world().get::<Transform>(shape).expect("bounds transform missing");
            let world = parent.mul_transform(*child);
            let (height, rotation) = if mode == BoundsMode::Hitbox {
                (physics.hitbox.center_y_offset(), Quat::from_rotation_y(-1.3))
            } else {
                (
                    physics.movement_collider.height / 2.0 + CHARACTER_CONTACT_OFFSET,
                    Quat::IDENTITY,
                )
            };
            assert!(world.translation.abs_diff_eq(Vec3::new(3.0 + alpha, height, 0.0), 1e-5));
            assert!(world.rotation.abs_diff_eq(rotation, 1e-5));
            assert!(world.scale.abs_diff_eq(Vec3::ONE, 1e-5));
        }
    }
}
