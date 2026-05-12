use bevy::prelude::*;

use crate::{config::AssetSet, constants::*};
use common::{
    constants::BARRIER_THICKNESS,
    protocol::{BarrierKindId, BarrierKindTable},
};

// Per-kind shared mesh + material handles. One mesh covers every barrier
// regardless of kind (variable length is handled by per-instance
// Transform.scale in `spawn.rs`). One material per kind, indexed by
// `BarrierKindId.0`, so all barriers of the same kind share a single handle
// and Bevy's automatic batching collapses N draws into one.
//
// All materials of the same kind also share the pulsation: the pulsate
// system mutates each material once per frame, propagating to every
// barrier (and matching-color key) of that kind.
#[derive(Resource)]
pub struct BarrierAssets {
    pub(super) mesh: Handle<Mesh>,
    pub(super) materials: Vec<Handle<StandardMaterial>>,
    // Mirror of the table at construction time, so the pulsate system can
    // re-derive the base color without re-reading the config every frame.
    pub(super) base_colors: Vec<Color>,
}

impl BarrierAssets {
    pub fn material_for(&self, kind: BarrierKindId) -> &Handle<StandardMaterial> {
        &self.materials[kind.0 as usize]
    }

    pub fn material_handles(&self) -> &[Handle<StandardMaterial>] {
        &self.materials
    }

    // sRGB base color for the kind, useful for HUD icons that aren't 3D
    // materials (e.g., a flat-shaded square).
    pub fn base_color(&self, kind: BarrierKindId) -> Color {
        self.base_colors[kind.0 as usize]
    }
}

pub fn setup_barrier_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    kind_table: Res<BarrierKindTable>,
    asset_set: Res<AssetSet>,
) {
    // Unit X and Y so per-instance `Transform.scale` can encode the merged
    // segment's length and barrier height. Thickness stays baked in the mesh
    // — no instance ever wants a different thickness.
    let mesh = meshes.add(Cuboid::new(1.0, 1.0, BARRIER_THICKNESS));

    let mut handles = Vec::with_capacity(kind_table.len());
    let mut base_colors = Vec::with_capacity(kind_table.len());
    for id in kind_table.ids() {
        let hex = asset_set
            .barrier_kind_color_hex(id)
            .expect("color presence checked at app startup");
        let color = parse_hex_color(hex).unwrap_or_else(|err| panic!("invalid color {hex:?} for kind {id:?}: {err}"));
        handles.push(materials.add(barrier_material(color)));
        base_colors.push(color);
    }

    commands.insert_resource(BarrierAssets {
        mesh,
        materials: handles,
        base_colors,
    });
}

fn barrier_material(color: Color) -> StandardMaterial {
    let linear = color.to_linear();
    StandardMaterial {
        base_color: Color::srgba(linear.red, linear.green, linear.blue, BARRIER_ALPHA_MAX),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        double_sided: true,
        cull_mode: None,
        ..default()
    }
}

// Parse "#rrggbb" or "rrggbb" into a Color in sRGB space.
fn parse_hex_color(hex: &str) -> Result<Color, String> {
    let h = hex.strip_prefix('#').unwrap_or(hex);
    if h.len() != 6 {
        return Err(format!("expected 6 hex digits, got {}", h.len()));
    }
    let parse_byte = |s: &str| u8::from_str_radix(s, 16).map_err(|e| e.to_string());
    let r = parse_byte(&h[0..2])? as f32 / 255.0;
    let g = parse_byte(&h[2..4])? as f32 / 255.0;
    let b = parse_byte(&h[4..6])? as f32 / 255.0;
    Ok(Color::srgb(r, g, b))
}
