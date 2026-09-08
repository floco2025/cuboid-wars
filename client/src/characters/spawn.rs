use bevy::{
    light::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
};
use common::{
    config::CharacterPhysicsConfig,
    constants::CHARACTER_CONTACT_OFFSET,
    physics::{
        CharacterSupport, CharacterVerticalVelocity, CollisionWorld, GroundingDiagnostics, PortalSet,
        grounding_diagnostics, passable_barrier_kinds,
    },
    protocol::{FaceYaw, PlateState, PlayerId, Position},
};

use super::BoundsMode;
use crate::{
    cameras::MainCameraMarker,
    players::{CuboidShake, PlayerMap},
};

#[derive(Component)]
pub struct CharacterBounds {
    physics: CharacterPhysicsConfig,
}

#[derive(Component)]
pub struct BoundsShapeMarker(BoundsMode);

pub fn spawn_character_bounds(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    physics: CharacterPhysicsConfig,
) -> Entity {
    let root = commands
        .spawn((CharacterBounds { physics }, Transform::default(), Visibility::Inherited))
        .id();
    let body = physics.movement_collider;
    let capsule = meshes.add(Capsule3d::new(body.radius(), body.height - body.diameter));
    let hitbox = meshes.add(Cuboid::new(
        physics.hitbox.width,
        physics.hitbox.height,
        physics.hitbox.depth,
    ));
    for (mode, mesh, color) in [
        (BoundsMode::Grounding, capsule, Color::srgba(0.1, 0.8, 1.0, 0.18)),
        (BoundsMode::Hitbox, hitbox, Color::srgba(1.0, 0.2, 0.2, 0.18)),
    ] {
        commands.spawn((
            BoundsShapeMarker(mode),
            ChildOf(root),
            Mesh3d(mesh),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::default(),
            Visibility::Hidden,
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
    root
}

pub fn character_bounds_sync_system(
    mode: Res<BoundsMode>,
    roots: Query<(&ChildOf, &CharacterBounds)>,
    actors: Query<(&FaceYaw, &Transform, Option<&CuboidShake>), Without<BoundsShapeMarker>>,
    mut shapes: Query<(&ChildOf, &BoundsShapeMarker, &mut Transform, &mut Visibility)>,
) {
    for (parent, marker, mut transform, mut visibility) in &mut shapes {
        let active = *mode != BoundsMode::Off && marker.0 == *mode;
        visibility.set_if_neq(if active {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if !active {
            continue;
        }
        let Ok((actor, bounds)) = roots.get(parent.parent()) else {
            continue;
        };
        let Ok((yaw, actor_transform, shake)) = actors.get(actor.parent()) else {
            continue;
        };
        let physics = bounds.physics;
        let origin = rendered_feet(actor_transform, shake);
        let (height, rotation) = match *mode {
            BoundsMode::Hitbox => (physics.hitbox.center_y_offset(), Quat::from_rotation_y(yaw.0)),
            _ => (
                physics.movement_collider.height / 2.0 + CHARACTER_CONTACT_OFFSET,
                Quat::IDENTITY,
            ),
        };
        *transform = bounds_transform(actor_transform, origin + Vec3::Y * height, rotation);
    }
}

fn rendered_feet(transform: &Transform, shake: Option<&CuboidShake>) -> Vec3 {
    transform.translation - shake.map_or(Vec3::ZERO, |shake| Vec3::new(shake.offset_x, 0.0, shake.offset_z))
}

fn bounds_transform(parent: &Transform, center: Vec3, rotation: Quat) -> Transform {
    Transform::from_translation(parent.compute_affine().inverse().transform_point3(center))
        .with_rotation(parent.rotation.inverse() * rotation)
        .with_scale(parent.scale.recip())
}

pub fn refresh_grounding_debug_system(
    mut commands: Commands,
    mode: Res<BoundsMode>,
    world: Res<CollisionWorld>,
    portals: Res<PortalSet>,
    plates: Res<PlateState>,
    players: Res<PlayerMap>,
    roots: Query<(&ChildOf, &CharacterBounds)>,
    actors: Query<(
        &Position,
        Option<&PlayerId>,
        &CharacterVerticalVelocity,
        Option<&GroundingDiagnostics>,
        Option<&CharacterSupport>,
    )>,
) {
    if *mode != BoundsMode::Grounding {
        return;
    }
    for (parent, bounds) in &roots {
        let entity = parent.parent();
        let Ok((pos, player, motion, cached, support)) = actors.get(entity) else {
            continue;
        };
        let origin = Vec3::from(*pos) + Vec3::Y * CHARACTER_CONTACT_OFFSET * 2.0;
        if cached.is_some_and(|g| g.origin.abs_diff_eq(origin, 1e-5)) {
            continue;
        }
        // Character blocking and portal hops can replace the motor's proposed position.
        let keys = player
            .and_then(|id| players.get(id))
            .map_or(&[][..], |p| p.held_keys.as_slice());
        let passable = passable_barrier_kinds(keys, &plates.open_barrier_kinds);
        let excluded = if player.is_some() {
            portals.collision_exclusions(Vec3::from(*pos), bounds.physics)
        } else {
            Vec::new()
        };
        let mut ground = grounding_diagnostics(&world, pos, bounds.physics, &passable, &excluded);
        ground.supported &= motion.0 <= 0.0;
        let support = if support == Some(&CharacterSupport::Ladder) && world.ladder_volume_at(pos).is_some() {
            CharacterSupport::Ladder
        } else if ground.supported {
            CharacterSupport::Ground
        } else {
            CharacterSupport::Airborne
        };
        commands.entity(entity).insert((ground, support));
    }
}

pub fn grounding_debug_system(
    mode: Res<BoundsMode>,
    roots: Query<(&ChildOf, &CharacterBounds)>,
    query: Query<(
        &Position,
        &Transform,
        Option<&CuboidShake>,
        &GroundingDiagnostics,
        Option<&CharacterSupport>,
    )>,
    cameras: Query<&GlobalTransform, With<MainCameraMarker>>,
    mut gizmos: Gizmos,
) {
    if *mode != BoundsMode::Grounding {
        return;
    }
    let camera_rotation = cameras.single().ok().map(GlobalTransform::rotation);
    for (parent, bounds) in &roots {
        let Ok((pos, transform, shake, ground, support)) = query.get(parent.parent()) else {
            continue;
        };
        let origin = rendered_feet(transform, shake);
        // Diagnostics are sampled at physics ticks; draw them in the interpolated body's frame.
        let render_offset = origin - Vec3::from(*pos);
        let support = support.copied().unwrap_or(if ground.supported {
            CharacterSupport::Ground
        } else {
            CharacterSupport::Airborne
        });
        let (label, color) = match support {
            CharacterSupport::Ground => ("Grounded", Color::srgb(0.1, 1.0, 0.3)),
            CharacterSupport::Airborne => ("Airborne", Color::srgb(1.0, 0.6, 0.1)),
            CharacterSupport::Ladder => ("Ladder", Color::srgb(0.1, 0.8, 1.0)),
        };
        if let Some(rotation) = camera_rotation {
            let body = bounds.physics.movement_collider;
            let center = origin + Vec3::Y * (body.height / 2.0) + rotation * Vec3::X * (body.radius() + 0.15);
            gizmos.text(
                Isometry3d::new(center, rotation),
                label,
                0.12,
                Vec2::new(-0.5, 0.0),
                color,
            );
        }
        let probe_origin = ground.origin + render_offset;
        gizmos.line(probe_origin, probe_origin - Vec3::Y * ground.distance, color);
        if let Some(hit) = ground.hit {
            let contact = hit.contact + render_offset;
            gizmos.sphere(Isometry3d::from_translation(contact), 0.035, color);
            gizmos.arrow(contact, contact + hit.normal * 0.35, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::input_bounds_cycle_system;

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
    fn keyboard_cycles_all_modes_for_existing_and_later_shapes() {
        let mut app = App::new();
        app.init_resource::<BoundsMode>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(
                Update,
                (input_bounds_cycle_system, character_bounds_sync_system).chain(),
            );
        let root = app.world_mut().spawn_empty().id();
        let first = app
            .world_mut()
            .spawn((
                ChildOf(root),
                BoundsShapeMarker(BoundsMode::Grounding),
                Transform::default(),
                Visibility::Hidden,
            ))
            .id();
        assert_eq!(*app.world().resource::<BoundsMode>(), BoundsMode::Off);
        for expected in [BoundsMode::Grounding, BoundsMode::Hitbox, BoundsMode::Off] {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::KeyB);
            app.update();
            assert_eq!(*app.world().resource::<BoundsMode>(), expected);
            assert_eq!(
                *app.world().get::<Visibility>(first).expect("bounds visibility missing"),
                if expected == BoundsMode::Grounding {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                }
            );
            app.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset_all();
            let later = app
                .world_mut()
                .spawn((
                    ChildOf(root),
                    BoundsShapeMarker(expected),
                    Transform::default(),
                    Visibility::Hidden,
                ))
                .id();
            app.update();
            assert_eq!(
                *app.world().get::<Visibility>(later).expect("bounds visibility missing"),
                if expected == BoundsMode::Off {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                }
            );
        }
    }
    #[test]
    fn bounds_interpolate_between_ticks_without_animation_or_hit_shake() {
        use crate::{characters::PreviousTickPosition, players::players_transform_sync_system};
        use common::{
            config::{HitboxConfig, MovementColliderConfig},
            protocol::PlayerMarker,
        };
        use std::time::Duration;

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
                    BoundsShapeMarker(mode),
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
}
