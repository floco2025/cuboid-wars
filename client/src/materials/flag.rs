use bevy::{
    asset::embedded_asset,
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

const FLAG_WIND_SHADER_PATH: &str = "embedded://client/materials/flag_wind.wgsl";

// A pennant hung from a pole: the mesh runs along +X from its hoist, the
// vertex shader turns it downwind and flutters it with the shared gust field,
// and the fragment shader weaves and hems the cloth.
pub type FlagMaterial = ExtendedMaterial<StandardMaterial, FlagWindExtension>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct FlagWindExtension {
    // xy = world wind direction (XZ), z = flutter amplitude at the tip (m), w = speed (rad/s)
    #[uniform(100)]
    pub wind: Vec4,
    // x = the length (m) over which the flutter ramps up, y = hem as a fraction
    // of the local height, z = hoist band as a fraction of the length, w = weave contrast
    #[uniform(101)]
    pub cloth: Vec4,
}

impl MaterialExtension for FlagWindExtension {
    fn vertex_shader() -> ShaderRef {
        FLAG_WIND_SHADER_PATH.into()
    }

    fn prepass_vertex_shader() -> ShaderRef {
        FLAG_WIND_SHADER_PATH.into()
    }

    fn deferred_vertex_shader() -> ShaderRef {
        FLAG_WIND_SHADER_PATH.into()
    }

    fn fragment_shader() -> ShaderRef {
        FLAG_WIND_SHADER_PATH.into()
    }

    fn prepass_fragment_shader() -> ShaderRef {
        FLAG_WIND_SHADER_PATH.into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        FLAG_WIND_SHADER_PATH.into()
    }
}

// Depends on `GrassMaterialPlugin`, which loads the shared wind module.
pub struct FlagMaterialPlugin;

impl Plugin for FlagMaterialPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "flag_wind.wgsl");
        app.add_plugins(MaterialPlugin::<FlagMaterial>::default());
    }
}
