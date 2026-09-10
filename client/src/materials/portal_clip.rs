use bevy::{
    asset::embedded_asset,
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

const PORTAL_CLIP_SHADER_PATH: &str = "embedded://client/materials/portal_clip.wgsl";

pub type PortalClipMaterial = ExtendedMaterial<StandardMaterial, PortalClipExtension>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct PortalClipExtension {
    // xyz = plane normal, w = -normal · plane point; fragments with a negative signed distance are discarded.
    #[uniform(100)]
    pub plane: Vec4,
}

impl MaterialExtension for PortalClipExtension {
    fn fragment_shader() -> ShaderRef {
        PORTAL_CLIP_SHADER_PATH.into()
    }

    fn prepass_fragment_shader() -> ShaderRef {
        PORTAL_CLIP_SHADER_PATH.into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        PORTAL_CLIP_SHADER_PATH.into()
    }
}

// `base` clipped to the half-space in front of `plane`. The mask alpha mode
// is what makes the depth-only prepass and the shadow passes run the
// fragment stage at all (Bevy skips it for opaque materials); the zero
// cutoff keeps the base texture's alpha inert.
#[must_use]
pub fn portal_clip_material(base: &StandardMaterial, plane: Vec4) -> PortalClipMaterial {
    PortalClipMaterial {
        base: StandardMaterial {
            alpha_mode: AlphaMode::Mask(0.0),
            ..base.clone()
        },
        extension: PortalClipExtension { plane },
    }
}

pub struct PortalClipMaterialPlugin;

impl Plugin for PortalClipMaterialPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "portal_clip.wgsl");
        app.add_plugins(MaterialPlugin::<PortalClipMaterial>::default());
    }
}
