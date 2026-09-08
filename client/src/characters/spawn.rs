use bevy::{
    light::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
};
use common::config::CharacterPhysicsConfig;

use super::ColliderBoxesVisible;

#[derive(Component)]
pub struct ColliderBoxMarker;

pub fn spawn_collider_box(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    physics: CharacterPhysicsConfig,
) -> Entity {
    commands
        .spawn((
            ColliderBoxMarker,
            Mesh3d(meshes.add(Cuboid::new(
                physics.collider.width,
                physics.collision_height(),
                physics.collider.depth,
            ))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.8, 0.2, 0.2, 0.18),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_translation(Vec3::ZERO),
            Visibility::Hidden,
            NotShadowCaster,
            NotShadowReceiver,
        ))
        .id()
}

pub fn collider_box_sync_system(
    visible: Res<ColliderBoxesVisible>,
    mut boxes: Query<&mut Visibility, With<ColliderBoxMarker>>,
) {
    for mut visibility in &mut boxes {
        let target = if visible.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        // Equal visibility writes would wake Bevy's visibility propagation.
        visibility.set_if_neq(target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::input_collider_boxes_toggle_system;

    #[test]
    fn keyboard_toggle_updates_existing_and_later_boxes_that_follow_parent_rotation() {
        let mut app = App::new();
        app.add_plugins(TransformPlugin)
            .init_resource::<ColliderBoxesVisible>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(
                Update,
                (input_collider_boxes_toggle_system, collider_box_sync_system).chain(),
            );
        let rotation = Quat::from_rotation_y(0.8);
        let parent = app.world_mut().spawn(Transform::from_rotation(rotation)).id();
        let first = app
            .world_mut()
            .spawn((
                ColliderBoxMarker,
                ChildOf(parent),
                Transform::default(),
                Visibility::Hidden,
            ))
            .id();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyB);
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(first).expect("box visibility missing"),
            Visibility::Inherited
        );
        let transform = app
            .world()
            .get::<GlobalTransform>(first)
            .expect("box transform missing");
        assert!(transform.rotation().abs_diff_eq(rotation, 1e-5));
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset_all();
        let later = app
            .world_mut()
            .spawn((
                ColliderBoxMarker,
                ChildOf(parent),
                Transform::default(),
                Visibility::Hidden,
            ))
            .id();
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(later).expect("box visibility missing"),
            Visibility::Inherited
        );
        for yaw in [-1.2, 2.4] {
            let rotation = Quat::from_rotation_y(yaw);
            app.world_mut()
                .get_mut::<Transform>(parent)
                .expect("parent transform missing")
                .rotation = rotation;
            app.update();
            for entity in [first, later] {
                let transform = app
                    .world()
                    .get::<GlobalTransform>(entity)
                    .expect("box transform missing");
                assert!(transform.rotation().abs_diff_eq(rotation, 1e-5));
            }
        }
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyB);
        app.update();
        for entity in [first, later] {
            assert_eq!(
                *app.world().get::<Visibility>(entity).expect("box visibility missing"),
                Visibility::Hidden
            );
        }
    }
}
