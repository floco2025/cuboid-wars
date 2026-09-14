use super::{burn::GrassBurn, spawn::GrassPatch};
use crate::{
    constants::{
        EXPLOSION_GRASS_BURN_CENTER_HEIGHT_FACTOR, EXPLOSION_GRASS_BURN_CENTER_SWAY_FACTOR,
        EXPLOSION_GRASS_BURN_CENTER_WIDTH_FACTOR, EXPLOSION_GRASS_BURN_COLOR, EXPLOSION_GRASS_BURN_MAX_COLOR_BLEND,
        EXPLOSION_GRASS_BURN_MID_BRIGHTNESS_FACTOR, EXPLOSION_GRASS_BURN_ROOT_BRIGHTNESS_FACTOR,
        EXPLOSION_GRASS_BURN_TIP_BRIGHTNESS_FACTOR, TERRAIN_GRASS_DRY, TERRAIN_GRASS_MID_DENSITY,
        TERRAIN_GRASS_NEAR_DENSITY,
    },
    map::terrain_surface::TerrainCover,
};
use bevy::{asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology};
use common::protocol::Floor;
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
// exceeds the blade half-width). Used for conservative visibility and burn
// bounds; blade bases themselves are clipped against the exact compiled
// terrain footprint, while flexible tips may naturally lean past an edge.
pub(super) const BLADE_MAX_OVERHANG: f32 = TUFT_RADIUS + BLADE_TIP_LEAN_MAX;
// The ripple term in `grass_wind.wgsl` adds 0.4x on top of the primary gust.
pub(in crate::map) const WIND_SWAY_FACTOR: f32 = 1.4;
pub(in crate::map) const AABB_BASE_PAD: f32 = 0.01;

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

    pub(in crate::map) fn tuft_count(self, area: f32) -> usize {
        (self.density() * area).round() as usize
    }
}

pub(super) fn grass_patch_mesh(
    patch: GrassPatch,
    footprint: &[Floor],
    lod: GrassLod,
    green: Color,
    burns: &[GrassBurn],
) -> Mesh {
    let candidate_count = patch_tuft_count(lod, patch);
    grass_scatter_mesh(
        patch_seed(patch),
        candidate_count,
        lod,
        green,
        burns,
        |rng| {
            Some(Vec3::new(
                rng.random_range(patch.x1..=patch.x2),
                patch.y,
                rng.random_range(patch.z1..=patch.z2),
            ))
        },
        |left, right| base_is_on_terrain(left, footprint, patch) && base_is_on_terrain(right, footprint, patch),
    )
}

pub(in crate::map) fn grass_scatter_mesh(
    seed: u64,
    candidate_count: usize,
    lod: GrassLod,
    green: Color,
    burns: &[GrassBurn],
    mut candidate: impl FnMut(&mut SmallRng) -> Option<Vec3>,
    base_allowed: impl Fn(Vec3, Vec3) -> bool,
) -> Mesh {
    let mut rng = SmallRng::seed_from_u64(seed);
    let vertex_count = candidate_count * lod.blades() * VERTICES_PER_BLADE;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(vertex_count);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(vertex_count);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(vertex_count);
    let mut indices: Vec<u32> = Vec::with_capacity(candidate_count * lod.blades() * INDICES_PER_BLADE);

    for _ in 0..candidate_count {
        let Some(tuft) = candidate(&mut rng) else { continue };
        let cover_position = Vec2::new(tuft.x, tuft.z);
        let cover = TerrainCover::at(cover_position);
        if rng.random::<f32>() > cover.grass_density(cover_position) {
            continue;
        }
        let base_color = Hsla::from(green.mix(&TERRAIN_GRASS_DRY, cover.dry * 0.55));
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
                root_angle.cos().mul_add(root_radius, tuft.x),
                tuft.y,
                root_angle.sin().mul_add(root_radius, tuft.z),
            );
            let yaw = rng.random_range(0.0..TAU);
            let half_width = rng.random_range(BLADE_HALF_WIDTH_MIN..=BLADE_HALF_WIDTH_MAX);
            let across = Vec3::new(yaw.cos(), 0.0, yaw.sin()) * half_width;
            if !base_allowed(root - across, root + across) {
                continue;
            }
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

            let base = u32::try_from(positions.len()).expect("grass mesh vertex count exceeds u32");
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
                grass_macro_color(
                    ring_color(hue, tuft_saturation, lightness, ROOT_LIGHTNESS_SCALE),
                    cover.grass_macro,
                ),
                burn_strength,
                EXPLOSION_GRASS_BURN_ROOT_BRIGHTNESS_FACTOR,
            );
            let mid_color = burned_color(
                grass_macro_color(
                    ring_color(hue, tuft_saturation, lightness, MID_LIGHTNESS_SCALE),
                    cover.grass_macro,
                ),
                burn_strength,
                EXPLOSION_GRASS_BURN_MID_BRIGHTNESS_FACTOR,
            );
            colors.push(root_color);
            colors.push(root_color);
            colors.push(mid_color);
            colors.push(mid_color);
            colors.push(burned_color(
                grass_macro_color(
                    ring_color(hue, tuft_saturation, lightness, TIP_LIGHTNESS_SCALE),
                    cover.grass_macro,
                ),
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

pub(super) fn patch_tuft_count(lod: GrassLod, patch: GrassPatch) -> usize {
    lod.tuft_count(patch.area())
}

fn patch_seed(patch: GrassPatch) -> u64 {
    let quantize = |value: f32| (value * 1000.0).round() as i64 as u64;
    quantize(patch.x1)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(quantize(patch.x2).wrapping_mul(0xD6E8_FEB8_6659_FD93))
        .wrapping_add(quantize(patch.z1).wrapping_mul(0xC2B2_AE3D_27D4_EB4F))
        .wrapping_add(quantize(patch.z2).wrapping_mul(0xA24B_AED4_963E_E407))
        .wrapping_add(u64::from(patch.level).wrapping_mul(0x9FB2_1C65_1E98_DF25))
        .wrapping_add(u64::from(patch.carrier.0).wrapping_mul(0xDB4F_0B91_75AE_2165))
}

fn base_is_on_terrain(position: Vec3, footprint: &[Floor], patch: GrassPatch) -> bool {
    const EPSILON: f32 = 0.0001;
    footprint.iter().any(|floor| {
        if floor.carrier != patch.carrier || floor.level != patch.level || (floor.y - position.y).abs() > EPSILON {
            return false;
        }
        let (x1, x2, z1, z2) = floor.bounds_xz();
        position.x >= x1 - EPSILON
            && position.x <= x2 + EPSILON
            && position.z >= z1 - EPSILON
            && position.z <= z2 + EPSILON
    })
}

fn ring_color(hue: f32, saturation: f32, lightness: f32, lightness_scale: f32) -> [f32; 4] {
    Color::hsl(hue, saturation, (lightness * lightness_scale).min(0.95))
        .to_linear()
        .to_f32_array()
}

fn grass_macro_color(mut color: [f32; 4], macro_value: f32) -> [f32; 4] {
    let tint = Vec3::new(0.92, 0.98, 1.04).lerp(Vec3::new(1.09, 1.03, 0.88), macro_value);
    color[0] *= tint.x;
    color[1] *= tint.y;
    color[2] *= tint.z;
    color
}

fn burned_color(color: [f32; 4], strength: f32, brightness: f32) -> [f32; 4] {
    let burned = EXPLOSION_GRASS_BURN_COLOR.to_linear().to_f32_array();
    let blend = strength * EXPLOSION_GRASS_BURN_MAX_COLOR_BLEND;
    // A configurable healthy green can be darker than the old fixed grass
    // color. Cap the charcoal target relative to that source so a burn is
    // always visibly darker instead of accidentally brightening dark grass.
    let target = |channel: usize| (burned[channel] * brightness).min(color[channel] * 0.18);
    [
        color[0] + (target(0) - color[0]) * blend,
        color[1] + (target(1) - color[1]) * blend,
        color[2] + (target(2) - color[2]) * blend,
        color[3],
    ]
}
