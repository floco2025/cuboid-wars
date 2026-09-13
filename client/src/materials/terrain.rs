use bevy::{
    asset::embedded_asset,
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

use super::standard::load_texture;
use crate::constants::{TERRAIN_GRASS_TILE_SIZE, TERRAIN_RELIEF};

const TERRAIN_SHADER: &str = "embedded://client/materials/terrain.wgsl";

pub type TerrainMaterial = ExtendedMaterial<StandardMaterial, TerrainExtension>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct TerrainExtension {
    #[uniform(100)]
    pub surface: Vec4,
    #[texture(101)]
    #[sampler(102)]
    pub grass: Handle<Image>,
}

impl MaterialExtension for TerrainExtension {
    fn fragment_shader() -> ShaderRef {
        TERRAIN_SHADER.into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        TERRAIN_SHADER.into()
    }
}

pub fn terrain_material(server: &AssetServer, anisotropy: u16, mipmaps: bool) -> TerrainMaterial {
    TerrainMaterial {
        base: StandardMaterial {
            perceptual_roughness: 0.95,
            reflectance: 0.1,
            ..default()
        },
        extension: TerrainExtension {
            surface: Vec4::new(TERRAIN_GRASS_TILE_SIZE, TERRAIN_RELIEF, 0.0, 0.0),
            grass: load_texture(
                server,
                "textures/meadow/meadow-albedo.png",
                true,
                false,
                anisotropy,
                mipmaps,
            ),
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
