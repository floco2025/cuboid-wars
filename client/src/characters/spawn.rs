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
    parents: Query<&Transform, Without<ColliderBoxMarker>>,
    mut boxes: Query<(&ChildOf, &mut Transform, &mut Visibility), With<ColliderBoxMarker>>,
) {
    for (parent, mut transform, mut visibility) in &mut boxes {
        let target = if visible.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        // Equal visibility writes would wake Bevy's visibility propagation.
        visibility.set_if_neq(target);
        if visible.0
            && let Ok(parent) = parents.get(parent.parent())
        {
            // Character colliders stay axis-aligned while their models turn.
            transform.rotation = parent.rotation.inverse();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::input_collider_boxes_toggle_system;

    #[test]
    fn keyboard_toggle_updates_existing_and_later_boxes_without_rotating_the_collider() {
        let mut app = App::new();
        app.init_resource::<ColliderBoxesVisible>()
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
        let transform = app.world().get::<Transform>(first).expect("box transform missing");
        assert!((rotation * transform.rotation).abs_diff_eq(Quat::IDENTITY, 1e-5));
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
