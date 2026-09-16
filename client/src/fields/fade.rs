use bevy::prelude::*;
use common::{physics::FieldKind, protocol::PlateState};

use crate::{
    config::{ClientSettings, FieldVfxConfig},
    materials::FieldMaterial,
    vfx::color_with_alpha,
};

const FIELD_FADE_SNAP: f32 = 0.002;

// The pane material of every spawned barrier field and bridge surface group,
// each keyed by one member whose plate state stands for the whole surface.
#[derive(Resource, Default)]
pub struct FieldSurfaces(pub(crate) Vec<FieldSurface>);

pub struct FieldSurface {
    pub state: FieldKind,
    pub material: Handle<FieldMaterial>,
    pub base_color: Color,
}

impl FieldSurfaces {
    pub fn forget_barriers(&mut self) {
        self.0.retain(|surface| !matches!(surface.state, FieldKind::Barrier(_)));
    }

    pub fn forget_bridges(&mut self) {
        self.0.retain(|surface| !matches!(surface.state, FieldKind::Bridge(_)));
    }
}

pub(crate) fn fields_fade_system(
    time: Res<Time>,
    client_settings: Res<ClientSettings>,
    plates: Res<PlateState>,
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
            fade_target(&plates, surface.state, config),
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

// A closed barrier or powered bridge shows at `opacity`, a passable one at `passable_opacity`.
pub(crate) fn fade_target(plates: &PlateState, state: FieldKind, config: FieldVfxConfig) -> f32 {
    let solid = match state {
        FieldKind::Barrier(id) => !plates.open_barriers.contains(&id),
        FieldKind::Bridge(id) => plates.powered_bridges.contains(&id),
    };
    if solid { config.opacity } else { config.passable_opacity }
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
