use super::{burn::GrassBurn, patch::GrassPatch};
use crate::{
    constants::{
        EXPLOSION_GRASS_BURN_CENTER_HEIGHT_FACTOR, EXPLOSION_GRASS_BURN_CENTER_SWAY_FACTOR,
        EXPLOSION_GRASS_BURN_CENTER_WIDTH_FACTOR, EXPLOSION_GRASS_BURN_COLOR, EXPLOSION_GRASS_BURN_MAX_COLOR_BLEND,
        EXPLOSION_GRASS_BURN_MID_BRIGHTNESS_FACTOR, EXPLOSION_GRASS_BURN_ROOT_BRIGHTNESS_FACTOR,
        EXPLOSION_GRASS_BURN_TIP_BRIGHTNESS_FACTOR, GRASS_DRY, GRASS_MID_DENSITY, GRASS_NEAR_DENSITY,
    },
    map::terrain::TerrainCover,
};
use bevy::{asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology};
use common::protocol::Floor;
use rand::{RngExt, SeedableRng, rngs::SmallRng};
use std::f32::consts::TAU;

pub(super) const BLADES_PER_TUFT: usize = 4;
const TUFT_RADIUS: f32 = 0.12;
const BLADE_HEIGHT_MIN: f32 = 0.09;
pub(super) const BLADE_HEIGHT_MAX: f32 = 0.38;
// Raising the uniform draw to this power skews heights toward the minimum:
// a meadow is mostly short blades with a few tall ones standing out.
const BLADE_HEIGHT_SKEW: f32 = 1.7;
const BLADE_HALF_WIDTH_MIN: f32 = 0.010;
const BLADE_HALF_WIDTH_MAX: f32 = 0.018;
// Tip lean as a fraction of the blade's height, so short blades stay upright.
const BLADE_LEAN_MIN: f32 = 0.15;
const BLADE_LEAN_MAX: f32 = 0.55;
const BLADE_NORMAL_TILT: f32 = 0.4;
// Each blade is two stacked segments: a root quad tapering to a mid ring,
// then a triangle to the tip. The mid ring sits at these fractions of the
// tip's height/lean/width, so the blade arcs instead of hinging; its sway
// weight lands mid-bend after the shader squares it.
const MID_HEIGHT_FRACTION: f32 = 0.55;
const MID_LEAN_FRACTION: f32 = 0.45;
const MID_WIDTH_FRACTION: f32 = 0.6;
pub(super) const MID_SWAY_WEIGHT: f32 = 0.55;
// Blades are lit like the ground, so their colours are scaled copies of the
// ground's mean colour: dark at the root where a clump shades itself, a
// little lighter and yellower at the translucent tip. Their area-weighted
// mean stays near the ground colour, so blades fading out with distance do
// not shift the meadow's colour.
const ROOT_SCALE: f32 = 0.5;
const MID_SCALE: f32 = 0.9;
const TIP_SCALE: f32 = 1.1;
const TIP_STRAW_MIX: f32 = 0.12;
// Each tuft jitters around the terrain cover's healthy/dry blend and each
// blade jitters again within its tuft, so variation forms patches rather
// than blade-level confetti. Hue jitter pushes red and blue apart.
const TUFT_BRIGHTNESS_JITTER: f32 = 0.12;
const TUFT_HUE_JITTER: f32 = 0.08;
const BLADE_BRIGHTNESS_JITTER: f32 = 0.06;
pub(super) const VERTICES_PER_BLADE: usize = 5;
pub(super) const INDICES_PER_BLADE: usize = 9;
// Widest horizontal reach of any vertex from its tuft center (tip lean
// exceeds the blade half-width). Used for conservative visibility and burn
// bounds; blade bases themselves are clipped against the exact compiled
// terrain footprint, while flexible tips may naturally lean past an edge.
pub(super) const BLADE_MAX_OVERHANG: f32 = TUFT_RADIUS + BLADE_HEIGHT_MAX * BLADE_LEAN_MAX;
// The farthest the wind carries a tip, in amplitudes: the 1.4x swing (primary
// plus ripple) and the 1.5x gust lean in `grass_wind.wgsl`.
pub(in crate::map) const WIND_SWAY_FACTOR: f32 = 2.9;
pub(in crate::map) const AABB_BASE_PAD: f32 = 0.01;

// Positions are carrier-local. This untextured material uses UV0 for
// sway weight (0 root / 1 tip) and per-blade phase instead of texture tiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum GrassLod {
    Near,
    Mid,
}

impl GrassLod {
    fn density(self) -> f32 {
        match self {
            Self::Near => GRASS_NEAR_DENSITY,
            Self::Mid => GRASS_MID_DENSITY,
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
    let candidate_count = lod.tuft_count(patch.area());
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
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(vertex_count);
    let mut indices: Vec<u32> = Vec::with_capacity(candidate_count * lod.blades() * INDICES_PER_BLADE);

    let dry = GRASS_DRY.to_linear().to_vec3();
    for _ in 0..candidate_count {
        let Some(tuft) = candidate(&mut rng) else { continue };
        let cover_position = Vec2::new(tuft.x, tuft.z);
        let cover = TerrainCover::at(cover_position);
        if rng.random::<f32>() > cover.grass_density(cover_position) {
            continue;
        }
        let tuft_brightness = 1.0 + rng.random_range(-TUFT_BRIGHTNESS_JITTER..=TUFT_BRIGHTNESS_JITTER);
        let tuft_hue = rng.random_range(-TUFT_HUE_JITTER..=TUFT_HUE_JITTER);
        let tuft_color = green.to_linear().to_vec3().lerp(dry, cover.dry * 0.35)
            * cover.shade
            * (0.92 + 0.16 * cover.grass_macro)
            * tuft_brightness
            * Vec3::new(1.0 + tuft_hue, 1.0, 1.0 - tuft_hue);
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
            let height =
                BLADE_HEIGHT_MIN + (BLADE_HEIGHT_MAX - BLADE_HEIGHT_MIN) * rng.random::<f32>().powf(BLADE_HEIGHT_SKEW);
            let lean_angle = rng.random_range(0.0..TAU);
            let lean = height * rng.random_range(BLADE_LEAN_MIN..=BLADE_LEAN_MAX);
            let lean_offset = Vec3::new(lean_angle.cos() * lean, 0.0, lean_angle.sin() * lean);
            // Tilted toward the lean, so blades facing the sun catch it and
            // the others fall off, instead of every blade shading like the
            // ground beneath it.
            let normal = (Vec3::Y + Vec3::new(lean_angle.cos(), 0.0, lean_angle.sin()) * BLADE_NORMAL_TILT)
                .normalize()
                .to_array();
            let phase = rng.random_range(0.0..1.0);
            let blade_color = tuft_color * (1.0 + rng.random_range(-BLADE_BRIGHTNESS_JITTER..=BLADE_BRIGHTNESS_JITTER));
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
            normals.extend([normal; VERTICES_PER_BLADE]);
            uvs.push([0.0, phase]);
            uvs.push([0.0, phase]);
            uvs.push([MID_SWAY_WEIGHT * sway_scale, phase]);
            uvs.push([MID_SWAY_WEIGHT * sway_scale, phase]);
            uvs.push([sway_scale, phase]);
            let root_color = burned_color(
                blade_color * ROOT_SCALE,
                burn_strength,
                EXPLOSION_GRASS_BURN_ROOT_BRIGHTNESS_FACTOR,
            );
            let mid_color = burned_color(
                blade_color * MID_SCALE,
                burn_strength,
                EXPLOSION_GRASS_BURN_MID_BRIGHTNESS_FACTOR,
            );
            let tip_color = burned_color(
                (blade_color * TIP_SCALE).lerp(dry * TIP_SCALE, TIP_STRAW_MIX),
                burn_strength,
                EXPLOSION_GRASS_BURN_TIP_BRIGHTNESS_FACTOR,
            );
            colors.push(root_color);
            colors.push(root_color);
            colors.push(mid_color);
            colors.push(mid_color);
            colors.push(tip_color);
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

    // Burns rebuild a chunk from its patches, never from the old mesh, so
    // nothing needs the main-world copy once the GPU has it.
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
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

fn burned_color(color: Vec3, strength: f32, brightness: f32) -> [f32; 4] {
    let burned = EXPLOSION_GRASS_BURN_COLOR.to_linear().to_vec3();
    let blend = strength * EXPLOSION_GRASS_BURN_MAX_COLOR_BLEND;
    // A configurable healthy green can be darker than the charcoal colour.
    // Cap the target relative to the source so a burn is always visibly
    // darker instead of accidentally brightening dark grass.
    let target = (burned * brightness).min(color * 0.18);
    color.lerp(target, blend).extend(1.0).to_array()
}
