use bevy::prelude::*;
use common::protocol::{FieldId, SwitchState};

use crate::{
    config::{ClientSettings, FieldVfxConfig},
    materials::FieldMaterial,
    vfx::color_with_alpha,
};

const FIELD_FADE_SNAP: f32 = 0.002;

// The pane material of every spawned barrier and bridge surface, each fading
// with its field.
#[derive(Resource, Default)]
pub struct FieldSurfaces(pub(crate) Vec<FieldSurface>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldPiece {
    Barrier,
    Bridge,
}

pub struct FieldSurface {
    pub field: FieldId,
    pub piece: FieldPiece,
    pub material: Handle<FieldMaterial>,
    pub base_color: Color,
}

impl FieldSurfaces {
    // A respawn of one kind of piece replaces its surfaces and keeps the other's.
    pub fn forget(&mut self, piece: FieldPiece) {
        self.0.retain(|surface| surface.piece != piece);
    }
}

pub(crate) fn fields_fade_system(
    time: Res<Time>,
    client_settings: Res<ClientSettings>,
    switch_state: Res<SwitchState>,
    surfaces: Res<FieldSurfaces>,
    mut materials: ResMut<Assets<FieldMaterial>>,
) {
    let config = client_settings.vfx.fields;
    for surface in &surfaces.0 {
        let Some(alpha) = materials
            .get(&surface.material)
            .map(|material| material.base.base_color.alpha())
        else {
            continue;
        };
        let Some(next) = fade_step(
            alpha,
            fade_target(&switch_state, surface.field, config),
            time.delta_secs(),
            config.fade_secs,
        ) else {
            continue;
        };
        // `get_mut` marks the asset modified and re-extracts it to the GPU,
        // so a settled surface is left untouched.
        if let Some(mut material) = materials.get_mut(&surface.material) {
            material.base.base_color = color_with_alpha(surface.base_color, next);
        }
    }
}

// A field that is on shows at `opacity`, one that is off at `passable_opacity`.
pub(crate) fn fade_target(switch_state: &SwitchState, field: FieldId, config: FieldVfxConfig) -> f32 {
    if switch_state.open_fields.contains(&field) {
        config.passable_opacity
    } else {
        config.opacity
    }
}

// Frame-rate independent easing; `None` once settled on the target.
fn fade_step(alpha: f32, target: f32, delta_secs: f32, fade_secs: f32) -> Option<f32> {
    if (alpha - target).abs() <= f32::EPSILON {
        return None;
    }
    let mut next = alpha;
    next.smooth_nudge(&target, 1.0 / fade_secs, delta_secs);
    Some(if (next - target).abs() < FIELD_FADE_SNAP {
        target
    } else {
        next
    })
}

#[cfg(test)]
#[path = "tests/fade.rs"]
mod tests;
