use bevy::prelude::*;

use crate::{
    config::{AssetSet, ClientSettings},
    constants::*,
    vfx::with_white_vertex_colors,
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
    // Per-kind opaque variant of the barrier material, used by pressure
    // plates. Same color, but `AlphaMode::Opaque` + not pulsated — a plate
    // is a fixed marker on the floor, not a translucent force-field.
    // Indexed by `BarrierKindId.0` like `materials`.
    pub(super) plate_materials: Vec<Handle<StandardMaterial>>,
    // Mirror of the table at construction time, so the pulsate system can
    // re-derive the base color without re-reading the config every frame.
    pub(super) base_colors: Vec<Color>,
}

impl BarrierAssets {
    pub fn material_for(&self, kind: BarrierKindId) -> &Handle<StandardMaterial> {
        &self.materials[kind.0 as usize]
    }

    pub fn material_for_plate(&self, kind: BarrierKindId) -> &Handle<StandardMaterial> {
        &self.plate_materials[kind.0 as usize]
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

pub fn setup_barrier_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    kind_table: Res<BarrierKindTable>,
    asset_set: Res<AssetSet>,
    _client_settings: Res<ClientSettings>,
) {
    let alpha_max = BARRIER_ALPHA_MAX;
    let emissive = BARRIER_EMISSIVE;
    // Barrier mesh: unit X and Y so per-instance `Transform.scale` can
    // encode the merged segment's length and barrier height. Thickness
    // stays baked in the mesh — no instance ever wants a different thickness.
    // Both meshes carry white vertex colors: the material is lit translucent
    // (emissive needs a lit material), and only the vertex-color Blend
    // permutation renders correctly in this app.
    let mesh = meshes.add(with_white_vertex_colors(
        Cuboid::new(1.0, 1.0, BARRIER_THICKNESS).into(),
    ));
    // Key mesh: a small fixed-size cuboid, no per-instance scaling.
    let key_mesh = meshes.add(with_white_vertex_colors(
        Cuboid::new(KEY_WIDTH, KEY_HEIGHT, KEY_DEPTH).into(),
    ));

    let mut handles = Vec::with_capacity(kind_table.len());
    let mut plate_handles = Vec::with_capacity(kind_table.len());
    let mut base_colors = Vec::with_capacity(kind_table.len());
    for id in kind_table.ids() {
        let hex = asset_set
            .barrier_kind_color_hex(id)
            .expect("barrier kind color missing from config");
        let color = parse_hex_color(hex).unwrap_or_else(|err| panic!("invalid color {hex:?} for kind {id:?}: {err}"));
        handles.push(materials.add(barrier_material(color, alpha_max, emissive)));
        plate_handles.push(materials.add(plate_material(color)));
        base_colors.push(color);
    }

    // All three vectors are indexed by `BarrierKindId.0` — a length divergence
    // would mean a future contributor split the loops apart. Catch that here
    // instead of as an out-of-bounds panic at first lookup.
    assert_eq!(handles.len(), plate_handles.len());
    assert_eq!(handles.len(), base_colors.len());

    commands.insert_resource(BarrierAssets {
        key_mesh,
        mesh,
        materials: handles,
        plate_materials: plate_handles,
        base_colors,
    });
}

// Lit, not unlit: emissive is ignored on unlit materials. The emissive is
// set once here and never pulsed — the pulsate system only animates
// `base_color.alpha`.
fn barrier_material(color: Color, alpha_max: f32, emissive: f32) -> StandardMaterial {
    let linear = color.to_linear();
    StandardMaterial {
        base_color: Color::srgba(linear.red, linear.green, linear.blue, alpha_max),
        emissive: LinearRgba::rgb(linear.red * emissive, linear.green * emissive, linear.blue * emissive),
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    }
}

// Opaque sibling of `barrier_material` — flat, solid color, unlit. Plates
// share their kind's color with the barriers they control but read as a
// permanent floor marker rather than a force-field.
fn plate_material(color: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        alpha_mode: AlphaMode::Opaque,
        unlit: true,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_color_parses_with_and_without_hash() {
        let expected = Color::srgb(1.0, 0.0, 0.0);
        assert_eq!(parse_hex_color("#ff0000").expect("hash form parses"), expected);
        assert_eq!(parse_hex_color("ff0000").expect("bare form parses"), expected);
        let mixed = parse_hex_color("#0080FF").expect("mixed case parses");
        assert_eq!(mixed, Color::srgb(0.0, 128.0 / 255.0, 1.0));
    }

    #[test]
    fn hex_color_rejects_wrong_length_and_bad_digits() {
        assert!(parse_hex_color("#fff").is_err(), "3-digit shorthand is not supported");
        assert!(parse_hex_color("").is_err());
        assert!(parse_hex_color("#gggggg").is_err(), "non-hex digits must be rejected");
    }
}
