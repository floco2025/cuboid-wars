use bevy::{
    asset::embedded_asset,
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::{ShaderRef, load_shader_library},
};

const GRASS_WIND_SHADER_PATH: &str = "embedded://client/materials/grass_wind.wgsl";

pub type GrassMaterial = ExtendedMaterial<StandardMaterial, GrassWindExtension>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct GrassWindExtension {
    // xy = world wind direction (XZ), z = amplitude (m), w = speed (rad/s)
    #[uniform(100)]
    pub wind: Vec4,
}

impl MaterialExtension for GrassWindExtension {
    fn vertex_shader() -> ShaderRef {
        GRASS_WIND_SHADER_PATH.into()
    }

    fn prepass_vertex_shader() -> ShaderRef {
        GRASS_WIND_SHADER_PATH.into()
    }

    fn deferred_vertex_shader() -> ShaderRef {
        GRASS_WIND_SHADER_PATH.into()
    }

    fn enable_shadows() -> bool {
        false
    }
}

pub struct GrassMaterialPlugin;

impl Plugin for GrassMaterialPlugin {
    fn build(&self, app: &mut App) {
        // The gust field both the grass and the tree shaders import.
        load_shader_library!(app, "wind.wgsl");
        embedded_asset!(app, "grass_wind.wgsl");
        app.add_plugins(MaterialPlugin::<GrassMaterial>::default());
    }
}
