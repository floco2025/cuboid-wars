use std::collections::HashSet;

use bevy::{
    light::NotShadowCaster,
    prelude::*,
    scene::{SceneInstance, SceneSpawner},
};
use common::config::CharacterPhysicsConfig;

use crate::config::RenderSettings;

// Marks the inner SceneRoot entity of a spawned character (the GLB model).
// Used by `character_shadow_settings_system` to find spawned scene instances
// and tweak their shadow casters.
#[derive(Component)]
pub struct CharacterModelMarker;

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

pub fn character_shadow_settings_system(
    mut commands: Commands,
    render_settings: Res<RenderSettings>,
    scene_spawner: Res<SceneSpawner>,
    character_models: Query<(Entity, &SceneInstance), With<CharacterModelMarker>>,
    mut processed: Local<HashSet<Entity>>,
) {
    if render_settings.shadows_player_enabled {
        return;
    }

    for (model_entity, scene_instance) in &character_models {
        if processed.contains(&model_entity) || !scene_spawner.instance_is_ready(**scene_instance) {
            continue;
        }

        for entity in scene_spawner.iter_instance_entities(**scene_instance) {
            commands.entity(entity).insert(NotShadowCaster);
        }
        processed.insert(model_entity);
    }
}
