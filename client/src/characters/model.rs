use bevy::{gltf::Gltf, prelude::*, world_serialization::WorldAssetRoot};

use crate::config::{ModelDef, gltf_path};

// A character model's GLB and which of its scenes to instantiate; the
// scene root is attached once the file has loaded.
#[derive(Component)]
pub struct CharacterModel {
    scene: String,
    scene_index: usize,
    gltf: Handle<Gltf>,
}

impl CharacterModel {
    // The clips of the loaded GLB, `None` until it has loaded.
    pub fn clips<'a>(&self, gltfs: &'a Assets<Gltf>) -> Option<&'a [Handle<AnimationClip>]> {
        gltfs.get(&self.gltf).map(|gltf| gltf.animations.as_slice())
    }

    pub fn scene(&self) -> &str {
        &self.scene
    }
}

// Request the GLB once and take its scene and clips from the loaded asset.
// Requesting `path#Scene0` and `path#Animation1` by label loads the file
// once per label still in flight, and every extra completion respawns the
// scene instance, undoing whatever the ready observers set up.
pub fn load_character_model(model: &ModelDef, server: &AssetServer) -> CharacterModel {
    CharacterModel {
        scene: model.scene.clone(),
        scene_index: model
            .scene_index()
            .expect("model scene reference lacks a #Scene<n> label"),
        gltf: server.load(gltf_path(&model.scene)),
    }
}

pub fn model_transform(model: &ModelDef) -> Transform {
    Transform::from_scale(Vec3::splat(model.scale))
        .with_rotation(Quat::from_rotation_x(model.x_rotation_degrees.to_radians()))
        .with_translation(Vec3::new(model.x_offset, model.y_offset, model.z_offset))
}

pub fn character_models_attach_system(
    mut commands: Commands,
    gltfs: Res<Assets<Gltf>>,
    pending: Query<(Entity, &CharacterModel), Without<WorldAssetRoot>>,
) {
    for (entity, model) in &pending {
        let Some(gltf) = gltfs.get(&model.gltf) else {
            continue;
        };
        match gltf.scenes.get(model.scene_index) {
            Some(scene) => {
                commands.entity(entity).insert(WorldAssetRoot(scene.clone()));
            }
            None => {
                error!("{} has no scene {}", model.scene, model.scene_index);
                commands.entity(entity).remove::<CharacterModel>();
            }
        }
    }
}
