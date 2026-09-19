use bevy::prelude::*;
use common::protocol::{FieldId, SwitchState};

use crate::{
    config::{ClientSettings, FieldVfxConfig},
    materials::FieldMaterial,
    vfx::color_with_alpha,
};

const FIELD_FADE_SNAP: f32 = 0.002;

// The pane material of every spawned barrier field and bridge surface group,
// each keyed by one member whose state stands for the whole surface.
#[derive(Resource, Default)]
pub struct FieldSurfaces(pub(crate) Vec<FieldSurface>);

pub struct FieldSurface {
    pub state: FieldId,
    pub material: Handle<FieldMaterial>,
    pub base_color: Color,
}

impl FieldSurfaces {
    pub fn forget_barriers(&mut self) {
        self.0.retain(|surface| !matches!(surface.state, FieldId::Barrier(_)));
    }

    pub fn forget_bridges(&mut self) {
        self.0.retain(|surface| !matches!(surface.state, FieldId::Bridge(_)));
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
            fade_target(&switch_state, surface.state, config),
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
pub(crate) fn fade_target(switch_state: &SwitchState, state: FieldId, config: FieldVfxConfig) -> f32 {
    if switch_state.open_fields.contains(&state) {
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
