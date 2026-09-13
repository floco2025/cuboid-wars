use super::{
    burn::GrassBurn,
    spawn::{OpenEdges, quantized_key},
};
use crate::{
    constants::{
        EXPLOSION_GRASS_BURN_CENTER_HEIGHT_FACTOR, EXPLOSION_GRASS_BURN_CENTER_SWAY_FACTOR,
        EXPLOSION_GRASS_BURN_CENTER_WIDTH_FACTOR, EXPLOSION_GRASS_BURN_COLOR, EXPLOSION_GRASS_BURN_MAX_COLOR_BLEND,
        EXPLOSION_GRASS_BURN_MID_BRIGHTNESS_FACTOR, EXPLOSION_GRASS_BURN_ROOT_BRIGHTNESS_FACTOR,
        EXPLOSION_GRASS_BURN_TIP_BRIGHTNESS_FACTOR, TERRAIN_GRASS_DRY, TERRAIN_GRASS_GREEN, TERRAIN_GRASS_MID_DENSITY,
        TERRAIN_GRASS_NEAR_DENSITY,
    },
    map::terrain_surface::TerrainCover,
};
use bevy::{asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology};
use common::protocol::TerrainCell;
use rand::{RngExt, SeedableRng, rngs::SmallRng};
use std::f32::consts::TAU;

pub(super) const BLADES_PER_TUFT: usize = 3;
const TUFT_RADIUS: f32 = 0.09;
const BLADE_HEIGHT_MIN: f32 = 0.1;
pub(super) const BLADE_HEIGHT_MAX: f32 = 0.3;
const BLADE_HALF_WIDTH_MIN: f32 = 0.008;
const BLADE_HALF_WIDTH_MAX: f32 = 0.015;
const BLADE_TIP_LEAN_MAX: f32 = 0.12;
// Each blade is two stacked segments: a root quad tapering to a mid ring,
// then a triangle to the tip. The mid ring sits at these fractions of the
// tip's height/lean/width, so the blade arcs instead of hinging; its sway
// weight lands mid-bend after the shader squares it.
const MID_HEIGHT_FRACTION: f32 = 0.55;
const MID_LEAN_FRACTION: f32 = 0.45;
const MID_WIDTH_FRACTION: f32 = 0.6;
pub(super) const MID_SWAY_WEIGHT: f32 = 0.55;
// Root-to-tip lightness ramp fakes the ambient occlusion inside a clump —
// flat-colored blades read as loose triangles, not grass.
const ROOT_LIGHTNESS_SCALE: f32 = 0.5;
const MID_LIGHTNESS_SCALE: f32 = 0.85;
const TIP_LIGHTNESS_SCALE: f32 = 1.2;
// Each tuft jitters around the terrain cover's healthy/dry blend and each
// blade jitters again within its tuft, so variation forms patches rather
// than blade-level confetti.
const TUFT_HUE_JITTER: f32 = 8.0;
const TUFT_SATURATION_JITTER: f32 = 0.05;
const TUFT_LIGHTNESS_JITTER: f32 = 0.05;
const BLADE_HUE_JITTER: f32 = 4.0;
const BLADE_LIGHTNESS_JITTER: f32 = 0.04;
pub(super) const VERTICES_PER_BLADE: usize = 5;
pub(super) const INDICES_PER_BLADE: usize = 9;
// Widest horizontal reach of any vertex from its tuft center (tip lean
// exceeds the blade half-width). Scatter runs to the cell border only toward
// neighbors that also have grass — contiguous cells tile seamlessly and the
// spill hides among the neighbor's blades (an inset there reads as a bare
// checkerboard grid). Every other edge (wall, bare floor, floor rim over a
// drop) is inset by this so no vertex leaves the cell.
pub(super) const BLADE_MAX_OVERHANG: f32 = TUFT_RADIUS + BLADE_TIP_LEAN_MAX;
// The ripple term in `grass_wind.wgsl` adds 0.4x on top of the primary gust.
pub(super) const WIND_SWAY_FACTOR: f32 = 1.4;
pub(super) const AABB_BASE_PAD: f32 = 0.01;

// Positions are carrier-local. This untextured material uses UV0 for
// sway weight (0 root / 1 tip) and per-blade phase instead of texture tiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GrassLod {
    Near,
    Mid,
}

impl GrassLod {
    fn density(self) -> f32 {
        match self {
            Self::Near => TERRAIN_GRASS_NEAR_DENSITY,
            Self::Mid => TERRAIN_GRASS_MID_DENSITY,
        }
    }

    fn blades(self) -> usize {
        match self {
            Self::Near => BLADES_PER_TUFT,
            Self::Mid => 2,
        }
    }
}

pub(super) fn grass_cell_mesh(
    cell: TerrainCell,
    cell_size: f32,
    lod: GrassLod,
    open: OpenEdges,
    burns: &[GrassBurn],
) -> Mesh {
    let mut rng = SmallRng::seed_from_u64(cell_seed(cell, cell_size));
    let candidate_count = cell_tuft_count(lod, cell_size);
    let vertex_count = candidate_count * lod.blades() * VERTICES_PER_BLADE;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(vertex_count);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(vertex_count);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(vertex_count);
    let mut indices: Vec<u32> = Vec::with_capacity(candidate_count * lod.blades() * INDICES_PER_BLADE);

    let half = cell_size / 2.0;
    let edge = |is_open: bool| if is_open { half } else { half - BLADE_MAX_OVERHANG };
    let (x_min, x_max) = (-edge(open.neg_x), edge(open.pos_x));
    let (z_min, z_max) = (-edge(open.neg_z), edge(open.pos_z));
    for _ in 0..candidate_count {
        let tuft_x = cell.x + rng.random_range(x_min..=x_max);
        let tuft_z = cell.z + rng.random_range(z_min..=z_max);
        let cover_position = Vec2::new(tuft_x, tuft_z);
        let cover = TerrainCover::at(cover_position);
        if rng.random::<f32>() > cover.grass_density(cover_position) {
            continue;
        }
        let base_color = Hsla::from(TERRAIN_GRASS_GREEN.mix(&TERRAIN_GRASS_DRY, cover.dry));
        let tuft_hue = base_color.hue + rng.random_range(-TUFT_HUE_JITTER..=TUFT_HUE_JITTER);
        let tuft_saturation = (base_color.saturation
            + rng.random_range(-TUFT_SATURATION_JITTER..=TUFT_SATURATION_JITTER))
        .clamp(0.0, 1.0);
        let tuft_lightness =
            base_color.lightness * cover.shade + rng.random_range(-TUFT_LIGHTNESS_JITTER..=TUFT_LIGHTNESS_JITTER);
        for _ in 0..lod.blades() {
            let root_angle = rng.random_range(0.0..TAU);
            let root_radius = rng.random_range(0.0..=TUFT_RADIUS);
            let root = Vec3::new(
                root_angle.cos().mul_add(root_radius, tuft_x),
                cell.y,
                root_angle.sin().mul_add(root_radius, tuft_z),
            );
            let yaw = rng.random_range(0.0..TAU);
            let half_width = rng.random_range(BLADE_HALF_WIDTH_MIN..=BLADE_HALF_WIDTH_MAX);
            let across = Vec3::new(yaw.cos(), 0.0, yaw.sin()) * half_width;
            let height = rng.random_range(BLADE_HEIGHT_MIN..=BLADE_HEIGHT_MAX);
            let lean_angle = rng.random_range(0.0..TAU);
            let lean = rng.random_range(0.0..=BLADE_TIP_LEAN_MAX);
            let lean_offset = Vec3::new(lean_angle.cos() * lean, 0.0, lean_angle.sin() * lean);
            let phase = rng.random_range(0.0..1.0);
            let hue = tuft_hue + rng.random_range(-BLADE_HUE_JITTER..=BLADE_HUE_JITTER);
            let lightness = tuft_lightness + rng.random_range(-BLADE_LIGHTNESS_JITTER..=BLADE_LIGHTNESS_JITTER);
            let burn_strength = burns.iter().map(|burn| burn.strength_at(root)).fold(0.0_f32, f32::max);
            let height_scale = 1.0 - burn_strength * (1.0 - EXPLOSION_GRASS_BURN_CENTER_HEIGHT_FACTOR);
            let width_scale = 1.0 - burn_strength * (1.0 - EXPLOSION_GRASS_BURN_CENTER_WIDTH_FACTOR);
            let sway_scale = 1.0 - burn_strength * (1.0 - EXPLOSION_GRASS_BURN_CENTER_SWAY_FACTOR);
            let across = across * width_scale;
            let mid = root
                + lean_offset * (MID_LEAN_FRACTION * height_scale)
                + Vec3::Y * (height * MID_HEIGHT_FRACTION * height_scale);
            let tip = root + lean_offset * height_scale + Vec3::Y * (height * height_scale);

            let base = u32::try_from(positions.len()).expect("grass cell vertex count exceeds u32");
            positions.push((root - across).to_array());
            positions.push((root + across).to_array());
            positions.push((mid - across * MID_WIDTH_FRACTION).to_array());
            positions.push((mid + across * MID_WIDTH_FRACTION).to_array());
            positions.push(tip.to_array());
            uvs.push([0.0, phase]);
            uvs.push([0.0, phase]);
            uvs.push([MID_SWAY_WEIGHT * sway_scale, phase]);
            uvs.push([MID_SWAY_WEIGHT * sway_scale, phase]);
            uvs.push([sway_scale, phase]);
            let root_color = burned_color(
                ring_color(hue, tuft_saturation, lightness, ROOT_LIGHTNESS_SCALE),
                burn_strength,
                EXPLOSION_GRASS_BURN_ROOT_BRIGHTNESS_FACTOR,
            );
            let mid_color = burned_color(
                ring_color(hue, tuft_saturation, lightness, MID_LIGHTNESS_SCALE),
                burn_strength,
                EXPLOSION_GRASS_BURN_MID_BRIGHTNESS_FACTOR,
            );
            colors.push(root_color);
            colors.push(root_color);
            colors.push(mid_color);
            colors.push(mid_color);
            colors.push(burned_color(
                ring_color(hue, tuft_saturation, lightness, TIP_LIGHTNESS_SCALE),
                burn_strength,
                EXPLOSION_GRASS_BURN_TIP_BRIGHTNESS_FACTOR,
            ));
            indices.extend([
                base,
                base + 1,
                base + 3,
                base,
                base + 3,
                base + 2,
                base + 2,
                base + 3,
                base + 4,
            ]);
        }
    }

    // Blades shade like the ground below them.
    let normals = vec![[0.0, 1.0, 0.0]; positions.len()];

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

pub(super) fn cell_tuft_count(lod: GrassLod, cell_size: f32) -> usize {
    (lod.density() * cell_size * cell_size).round() as usize
}

fn cell_seed(cell: TerrainCell, cell_size: f32) -> u64 {
    let (_, quantized_x, quantized_z, level) = quantized_key(cell, cell_size);
    (quantized_x as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((quantized_z as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F))
        .wrapping_add(u64::from(level))
}

fn ring_color(hue: f32, saturation: f32, lightness: f32, lightness_scale: f32) -> [f32; 4] {
    Color::hsl(hue, saturation, (lightness * lightness_scale).min(0.95))
        .to_linear()
        .to_f32_array()
}

fn burned_color(color: [f32; 4], strength: f32, brightness: f32) -> [f32; 4] {
    let burned = EXPLOSION_GRASS_BURN_COLOR.to_linear().to_f32_array();
    let blend = strength * EXPLOSION_GRASS_BURN_MAX_COLOR_BLEND;
    [
        color[0] + (burned[0] * brightness - color[0]) * blend,
        color[1] + (burned[1] * brightness - color[1]) * blend,
        color[2] + (burned[2] * brightness - color[2]) * blend,
        color[3],
    ]
}
