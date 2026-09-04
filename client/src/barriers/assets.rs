use bevy::prelude::*;

use crate::{
    config::{AssetSet, assets::parse_hex_color},
    constants::*,
    vfx::{translucent_kind_material, with_white_vertex_colors},
};
use common::{
    constants::BARRIER_THICKNESS,
    protocol::{BarrierKindId, BarrierKindTable},
};

// Mesh + materials for both barriers (full-size, scaled per-instance to
// match the segment length) and keys (a small rotating cuboid that reuses
// the matching barrier material, so the pulse is in sync).
//
// Keys share `key_mesh` across all kinds; their material comes from
// `materials[kind]` — same as the matching barrier. One material handle per
// kind = automatic batching for both barriers and keys of that kind.

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
    pub(super) key_mesh: Handle<Mesh>,
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

    pub fn key_mesh(&self) -> &Handle<Mesh> {
        &self.key_mesh
    }
}

pub fn build_barrier_assets(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    kind_table: &BarrierKindTable,
    asset_set: &AssetSet,
) -> BarrierAssets {
    // Barrier mesh: unit X and Y so per-instance `Transform.scale` can
    // encode the merged segment's length and barrier height. Thickness
    // stays baked in the mesh — no instance ever wants a different thickness.
    // Both meshes carry the white vertex colors `translucent_kind_material`
    // needs to render.
    let mesh = meshes.add(with_white_vertex_colors(
        Cuboid::new(1.0, 1.0, BARRIER_THICKNESS).into(),
    ));
    // Key mesh: a small fixed-size cuboid, no per-instance scaling.
    let key_mesh = meshes.add(with_white_vertex_colors(
        Cuboid::new(KEY_WIDTH, KEY_HEIGHT, KEY_DEPTH).into(),
    ));

    let mut handles = Vec::with_capacity(kind_table.len());
    let mut base_colors = Vec::with_capacity(kind_table.len());
    for id in kind_table.ids() {
        let hex = asset_set
            .barrier_kind_color_hex(id)
            .expect("barrier kind color missing from config");
        let color = parse_hex_color(hex).unwrap_or_else(|err| panic!("invalid color {hex:?} for kind {id:?}: {err}"));
        handles.push(materials.add(translucent_kind_material(color, BARRIER_ALPHA_MAX, BARRIER_EMISSIVE)));
        base_colors.push(color);
    }

    // Both vectors are indexed by `BarrierKindId.0` — a length divergence
    // would mean a future contributor split the loops apart. Catch that here
    // instead of as an out-of-bounds panic at first lookup.
    assert_eq!(handles.len(), base_colors.len());

    BarrierAssets {
        key_mesh,
        mesh,
        materials: handles,
        base_colors,
    }
}
