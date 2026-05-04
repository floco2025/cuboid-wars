use bevy::prelude::*;
use common::config::CharacterPhysicsConfig;

pub fn spawn_collider_box(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    physics: CharacterPhysicsConfig,
) -> Entity {
    commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(
                physics.collider.width,
                physics.collision_height(),
                physics.collider.depth,
            ))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.8, 0.2, 0.2, 0.5),
                alpha_mode: AlphaMode::Blend,
                ..default()
            })),
            Transform::from_translation(Vec3::ZERO),
        ))
        .id()
}
