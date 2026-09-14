use bevy::{
    asset::embedded_asset,
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

const TREE_WIND_SHADER_PATH: &str = "embedded://client/materials/tree_wind.wgsl";

pub type TreeMaterial = ExtendedMaterial<StandardMaterial, TreeWindExtension>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct TreeWindExtension {
    // xy = world wind direction (XZ), z = crown amplitude (m), w = speed (rad/s)
    #[uniform(100)]
    pub wind: Vec4,
}

impl MaterialExtension for TreeWindExtension {
    fn vertex_shader() -> ShaderRef {
        TREE_WIND_SHADER_PATH.into()
    }

    fn prepass_vertex_shader() -> ShaderRef {
        TREE_WIND_SHADER_PATH.into()
    }

    fn deferred_vertex_shader() -> ShaderRef {
        TREE_WIND_SHADER_PATH.into()
    }
}

// Depends on `GrassMaterialPlugin`, which loads the shared wind module.
pub struct TreeMaterialPlugin;

impl Plugin for TreeMaterialPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "tree_wind.wgsl");
        app.add_plugins(MaterialPlugin::<TreeMaterial>::default());
    }
}
