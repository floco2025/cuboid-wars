use bevy::prelude::*;

use crate::{
    config::AssetSet,
    constants::{BRIDGE_ALPHA_OFF, BRIDGE_EMISSIVE},
    vfx::with_white_vertex_colors,
};
use common::{
    constants::BRIDGE_THICKNESS,
    protocol::{BridgeKindId, BridgeKindTable},
};

// One unit slab for every bridge (per-instance `Transform.scale` encodes the
// rectangle) and one material per kind, indexed by `BridgeKindId.0`, so the
// fade system's single write per kind reaches every bridge of that kind.
#[derive(Resource)]
pub struct BridgeAssets {
    pub(super) mesh: Handle<Mesh>,
    pub(super) materials: Vec<Handle<StandardMaterial>>,
    // sRGB colors as configured; the fade system rebuilds `base_color` from
    // these with the current alpha.
    pub(super) base_colors: Vec<Color>,
}

impl BridgeAssets {
    pub fn material_for(&self, kind: BridgeKindId) -> &Handle<StandardMaterial> {
        &self.materials[usize::from(kind.0)]
    }

    pub fn material_handles(&self) -> &[Handle<StandardMaterial>] {
        &self.materials
    }

    pub fn base_color(&self, kind: BridgeKindId) -> Color {
        self.base_colors[usize::from(kind.0)]
    }
}

pub fn build_bridge_assets(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    kind_table: &BridgeKindTable,
    asset_set: &AssetSet,
) -> BridgeAssets {
    // Same recipe as barriers: a lit translucent material only renders on the
    // vertex-color Blend permutation in this app.
    let mesh = meshes.add(with_white_vertex_colors(Cuboid::new(1.0, BRIDGE_THICKNESS, 1.0).into()));

    let mut handles = Vec::with_capacity(kind_table.len());
    let mut base_colors = Vec::with_capacity(kind_table.len());
    for id in kind_table.ids() {
        let hex = asset_set
            .bridge_kind_color_hex(id)
            .expect("bridge kind color missing from config");
        let color = parse_hex_color(hex).unwrap_or_else(|err| panic!("invalid color {hex:?} for kind {id:?}: {err}"));
        handles.push(materials.add(bridge_material(color, BRIDGE_ALPHA_OFF)));
        base_colors.push(color);
    }
    assert_eq!(handles.len(), base_colors.len());

    BridgeAssets {
        mesh,
        materials: handles,
        base_colors,
    }
}

// Lit, not unlit: emissive is ignored on unlit materials. Only `base_color`
// changes afterwards (the fade system moves its alpha).
fn bridge_material(color: Color, alpha: f32) -> StandardMaterial {
    let linear = color.to_linear();
    StandardMaterial {
        base_color: bridge_base_color(color, alpha),
        emissive: LinearRgba::rgb(
            linear.red * BRIDGE_EMISSIVE,
            linear.green * BRIDGE_EMISSIVE,
            linear.blue * BRIDGE_EMISSIVE,
        ),
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    }
}

pub(super) fn bridge_base_color(color: Color, alpha: f32) -> Color {
    let linear = color.to_linear();
    Color::srgba(linear.red, linear.green, linear.blue, alpha)
}

fn parse_hex_color(hex: &str) -> Result<Color, String> {
    let h = hex.strip_prefix('#').unwrap_or(hex);
    if h.len() != 6 {
        return Err(format!("expected 6 hex digits, got {}", h.len()));
    }
    let parse_byte = |s: &str| u8::from_str_radix(s, 16).map_err(|e| e.to_string());
    let r = f32::from(parse_byte(&h[0..2])?) / 255.0;
    let g = f32::from(parse_byte(&h[2..4])?) / 255.0;
    let b = f32::from(parse_byte(&h[4..6])?) / 255.0;
    Ok(Color::srgb(r, g, b))
}
