use bevy::{
    asset::embedded_asset,
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

use crate::{
    constants::{
        FIELD_EDGE_GLOW, FIELD_FACE_OPACITY_RATIO, FIELD_FRESNEL_POWER, FIELD_HEX_CELL_SIZE, FIELD_HEX_LINE_GLOW,
        FIELD_HEX_LINE_WIDTH,
    },
    vfx::translucent_kind_material,
};

const FIELD_SHADER_PATH: &str = "embedded://client/materials/field.wgsl";

// The pane of a barrier or light bridge: the translucent kind material with a
// hex pattern in carrier-frame metres, a Fresnel glow toward grazing angles,
// and a band that brightens toward the frame. `base.base_color.alpha` still
// carries the fade between the solid and passable opacities.
pub type FieldMaterial = ExtendedMaterial<StandardMaterial, FieldExtension>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct FieldExtension {
    // x = hex cell width across flats (m), y = line width (m), z = line glow, w = Fresnel power.
    #[uniform(100)]
    pub pattern: Vec4,
    // x = head-on opacity as a fraction of the pane alpha, y = edge glow.
    #[uniform(101)]
    pub shape: Vec4,
}

impl MaterialExtension for FieldExtension {
    fn fragment_shader() -> ShaderRef {
        FIELD_SHADER_PATH.into()
    }

    fn enable_shadows() -> bool {
        false
    }
}

#[must_use]
pub fn field_material(color: Color, alpha: f32, emissive: f32) -> FieldMaterial {
    FieldMaterial {
        base: translucent_kind_material(color, alpha, emissive),
        extension: FieldExtension {
            pattern: Vec4::new(
                FIELD_HEX_CELL_SIZE,
                FIELD_HEX_LINE_WIDTH,
                FIELD_HEX_LINE_GLOW,
                FIELD_FRESNEL_POWER,
            ),
            shape: Vec4::new(FIELD_FACE_OPACITY_RATIO, FIELD_EDGE_GLOW, 0.0, 0.0),
        },
    }
}

pub struct FieldMaterialPlugin;

impl Plugin for FieldMaterialPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "field.wgsl");
        app.add_plugins(MaterialPlugin::<FieldMaterial>::default());
    }
}

#[cfg(test)]
#[path = "tests/field.rs"]
mod tests;
