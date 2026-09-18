use bevy::{
    asset::embedded_asset,
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

use super::{MaterialTextures, standard::load_texture};
use crate::{
    config::MaterialDef,
    constants::{TERRAIN_GRASS_TILE_SIZE, TERRAIN_RELIEF, TERRAIN_SOIL_RELIEF, TERRAIN_SOIL_TILE_SIZE},
};

const TERRAIN_SHADER: &str = "embedded://client/materials/terrain.wgsl";

pub type TerrainMaterial = ExtendedMaterial<StandardMaterial, TerrainExtension>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct TerrainExtension {
    #[uniform(100)]
    pub surface: Vec4,
    #[texture(101)]
    #[sampler(102)]
    pub grass: Handle<Image>,
    #[texture(103)]
    #[sampler(104)]
    pub soil: Handle<Image>,
    #[uniform(105)]
    pub grass_color: Vec4,
    // x wetness (0 dry .. 1 soaked)
    #[uniform(106)]
    pub weather: Vec4,
}

impl MaterialExtension for TerrainExtension {
    fn fragment_shader() -> ShaderRef {
        TERRAIN_SHADER.into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        TERRAIN_SHADER.into()
    }
}

// `definition` is the `procedural-terrain` entry in assets.json: it carries
// the surface response and the footstep sound, while the textures are fixed.
pub fn terrain_material(
    textures: &mut MaterialTextures,
    definition: &MaterialDef,
    anisotropy: u16,
    mipmaps: bool,
    grass_color: Color,
) -> TerrainMaterial {
    TerrainMaterial {
        base: StandardMaterial {
            perceptual_roughness: definition.perceptual_roughness,
            metallic: definition.metallic,
            reflectance: 0.1,
            ..default()
        },
        extension: TerrainExtension {
            surface: Vec4::new(
                TERRAIN_GRASS_TILE_SIZE,
                TERRAIN_RELIEF,
                TERRAIN_SOIL_RELIEF,
                TERRAIN_SOIL_TILE_SIZE,
            ),
            grass: load_texture(
                textures,
                "textures/meadow/meadow-albedo.png",
                true,
                false,
                anisotropy,
                mipmaps,
            ),
            soil: load_texture(
                textures,
                "textures/soil/soil-albedo.png",
                true,
                false,
                anisotropy,
                mipmaps,
            ),
            grass_color: Vec4::from_array(grass_color.to_linear().to_f32_array()),
            weather: Vec4::ZERO,
        },
    }
}

pub struct TerrainMaterialPlugin;

impl Plugin for TerrainMaterialPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "terrain.wgsl");
        app.add_plugins(MaterialPlugin::<TerrainMaterial>::default());
    }
}
