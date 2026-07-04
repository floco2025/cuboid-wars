use std::collections::HashSet;
use std::f32::consts::TAU;

use bevy::{
    asset::RenderAssetUsages, camera::primitives::Aabb, light::NotShadowCaster, mesh::Indices, prelude::*,
    render::render_resource::PrimitiveTopology,
};
use rand::{RngExt, SeedableRng, rngs::SmallRng};

use crate::{
    config::{ClientSettings, GrassConfig},
    map::MapLevel,
    materials::{GrassMaterial, GrassWindExtension},
};
use common::{
    constants::GRID_CELL_SIZE,
    protocol::{GrassCell, MapLayout},
};

const BLADES_PER_TUFT: usize = 6;
const TUFT_RADIUS: f32 = 0.09;
const BLADE_HEIGHT_MIN: f32 = 0.1;
const BLADE_HEIGHT_MAX: f32 = 0.3;
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
const MID_SWAY_WEIGHT: f32 = 0.55;
// Root-to-tip lightness ramp fakes the ambient occlusion inside a clump —
// flat-colored blades read as loose triangles, not grass.
const ROOT_LIGHTNESS_SCALE: f32 = 0.5;
const MID_LIGHTNESS_SCALE: f32 = 0.85;
const TIP_LIGHTNESS_SCALE: f32 = 1.2;
// Base grass color. Each tuft jitters around it and each blade jitters again
// within its tuft, so color varies patch-to-patch rather than blade-to-blade
// (confetti).
const BLADE_BASE_COLOR_RGB: [u8; 3] = [125, 173, 93];
const TUFT_HUE_JITTER: f32 = 8.0;
const TUFT_SATURATION_JITTER: f32 = 0.05;
const TUFT_LIGHTNESS_JITTER: f32 = 0.05;
const BLADE_HUE_JITTER: f32 = 4.0;
const BLADE_LIGHTNESS_JITTER: f32 = 0.04;
const VERTICES_PER_BLADE: usize = 5;
const INDICES_PER_BLADE: usize = 9;
// Widest horizontal reach of any vertex from its tuft center (tip lean
// exceeds the blade half-width). Scatter runs to the cell border only toward
// neighbors that also have grass — contiguous cells tile seamlessly and the
// spill hides among the neighbor's blades (an inset there reads as a bare
// checkerboard grid). Every other edge (wall, bare floor, floor rim over a
// drop) is inset by this so no vertex leaves the cell.
const BLADE_MAX_OVERHANG: f32 = TUFT_RADIUS + BLADE_TIP_LEAN_MAX;
// The ripple term in `grass_wind.wgsl` adds 0.4x on top of the primary gust.
const WIND_SWAY_FACTOR: f32 = 1.4;
const AABB_BASE_PAD: f32 = 0.01;

#[derive(Component)]
pub struct GrassMarker;

// Which cell edges border another grass cell on the same level; scatter may
// reach (and slightly overhang) the border only on those edges.
#[derive(Clone, Copy)]
struct OpenEdges {
    pos_x: bool,
    neg_x: bool,
    pos_z: bool,
    neg_z: bool,
}

impl OpenEdges {
    fn for_cell(cell: GrassCell, painted: &HashSet<(i64, i64, u8)>) -> Self {
        let (x, z, level) = quantized_key(cell);
        Self {
            pos_x: painted.contains(&(x + 2, z, level)),
            neg_x: painted.contains(&(x - 2, z, level)),
            pos_z: painted.contains(&(x, z + 2, level)),
            neg_z: painted.contains(&(x, z - 2, level)),
        }
    }
}

// Spawn one mesh entity per grass cell in the current `MapLayout`. Re-runs
// whenever `MapLayout` is inserted or replaced (e.g., reconnect / map change).
pub fn grass_spawn_system(
    mut commands: Commands,
    map_layout: Option<Res<MapLayout>>,
    client_settings: Res<ClientSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GrassMaterial>>,
    existing: Query<Entity, With<GrassMarker>>,
) {
    let Some(layout) = map_layout else { return };
    if !layout.is_changed() {
        return;
    }

    for entity in &existing {
        commands.entity(entity).despawn();
    }

    let grass = &client_settings.grass;
    if !grass.enabled || layout.grass.is_empty() {
        return;
    }

    let material = materials.add(grass_material(grass));
    let painted: HashSet<(i64, i64, u8)> = layout.grass.iter().map(|c| quantized_key(*c)).collect();

    for cell in layout.grass.iter().copied() {
        let open = OpenEdges::for_cell(cell, &painted);
        commands.spawn((
            GrassMarker,
            MapLevel(cell.level),
            Mesh3d(meshes.add(grass_cell_mesh(cell, grass, open))),
            MeshMaterial3d(material.clone()),
            Transform::default(),
            Visibility::Visible,
            // Belt-and-braces with `GrassWindExtension::enable_shadows()`.
            NotShadowCaster,
            grass_cell_aabb(cell, grass),
        ));
    }
}

fn grass_material(config: &GrassConfig) -> GrassMaterial {
    let wind_direction = Vec2::from_angle(config.wind_direction_degrees.to_radians());
    GrassMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.95,
            reflectance: 0.1,
            // Normals are +Y on both blade faces, so skip backface culling
            // instead of doing double-sided normal work.
            cull_mode: None,
            ..default()
        },
        extension: GrassWindExtension {
            wind: Vec4::new(
                wind_direction.x,
                wind_direction.y,
                config.wind_strength,
                config.wind_speed,
            ),
        },
    }
}

// Positions are world-space (entity at `Transform::default()`, matching the
// `MapGeometryBatch` convention). UV0 = (sway weight: 0 root / 1 tip,
// per-blade phase) — a deliberate exception to the world-position-UV
// convention, which exists for texture tiling; this material is untextured.
fn grass_cell_mesh(cell: GrassCell, config: &GrassConfig, open: OpenEdges) -> Mesh {
    let mut rng = SmallRng::seed_from_u64(cell_seed(cell));
    let tuft_count = cell_tuft_count(config);
    let vertex_count = tuft_count * BLADES_PER_TUFT * VERTICES_PER_BLADE;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(vertex_count);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(vertex_count);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(vertex_count);
    let mut indices: Vec<u32> = Vec::with_capacity(tuft_count * BLADES_PER_TUFT * INDICES_PER_BLADE);

    let half = GRID_CELL_SIZE / 2.0;
    let edge = |is_open: bool| if is_open { half } else { half - BLADE_MAX_OVERHANG };
    let (x_min, x_max) = (-edge(open.neg_x), edge(open.pos_x));
    let (z_min, z_max) = (-edge(open.neg_z), edge(open.pos_z));
    let base_color = Hsla::from(Color::srgb_u8(
        BLADE_BASE_COLOR_RGB[0],
        BLADE_BASE_COLOR_RGB[1],
        BLADE_BASE_COLOR_RGB[2],
    ));
    for _ in 0..tuft_count {
        let tuft_x = cell.x + rng.random_range(x_min..=x_max);
        let tuft_z = cell.z + rng.random_range(z_min..=z_max);
        let tuft_hue = base_color.hue + rng.random_range(-TUFT_HUE_JITTER..=TUFT_HUE_JITTER);
        let tuft_saturation = (base_color.saturation
            + rng.random_range(-TUFT_SATURATION_JITTER..=TUFT_SATURATION_JITTER))
        .clamp(0.0, 1.0);
        let tuft_lightness = base_color.lightness + rng.random_range(-TUFT_LIGHTNESS_JITTER..=TUFT_LIGHTNESS_JITTER);
        for _ in 0..BLADES_PER_TUFT {
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
            let mid = root + lean_offset * MID_LEAN_FRACTION + Vec3::Y * (height * MID_HEIGHT_FRACTION);
            let tip = root + lean_offset + Vec3::Y * height;
            let phase = rng.random_range(0.0..1.0);
            let hue = tuft_hue + rng.random_range(-BLADE_HUE_JITTER..=BLADE_HUE_JITTER);
            let lightness = tuft_lightness + rng.random_range(-BLADE_LIGHTNESS_JITTER..=BLADE_LIGHTNESS_JITTER);

            let base = u32::try_from(positions.len()).expect("grass cell vertex count exceeds u32");
            positions.push((root - across).to_array());
            positions.push((root + across).to_array());
            positions.push((mid - across * MID_WIDTH_FRACTION).to_array());
            positions.push((mid + across * MID_WIDTH_FRACTION).to_array());
            positions.push(tip.to_array());
            uvs.push([0.0, phase]);
            uvs.push([0.0, phase]);
            uvs.push([MID_SWAY_WEIGHT, phase]);
            uvs.push([MID_SWAY_WEIGHT, phase]);
            uvs.push([1.0, phase]);
            let root_color = ring_color(hue, tuft_saturation, lightness, ROOT_LIGHTNESS_SCALE);
            let mid_color = ring_color(hue, tuft_saturation, lightness, MID_LIGHTNESS_SCALE);
            colors.push(root_color);
            colors.push(root_color);
            colors.push(mid_color);
            colors.push(mid_color);
            colors.push(ring_color(hue, tuft_saturation, lightness, TIP_LIGHTNESS_SCALE));
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

fn cell_tuft_count(config: &GrassConfig) -> usize {
    (config.tufts_per_m2 * GRID_CELL_SIZE * GRID_CELL_SIZE).round() as usize
}

// Cell centers sit at odd multiples of `GRID_CELL_SIZE / 2`, so doubling
// before rounding recovers a stable integer independent of float noise —
// all clients render identical grass regardless of `Vec` ordering. Adjacent
// cells differ by exactly 2 in the quantized coordinate.
fn quantized_key(cell: GrassCell) -> (i64, i64, u8) {
    let quantized_x = (cell.x * 2.0 / GRID_CELL_SIZE).round() as i64;
    let quantized_z = (cell.z * 2.0 / GRID_CELL_SIZE).round() as i64;
    (quantized_x, quantized_z, cell.level)
}

fn cell_seed(cell: GrassCell) -> u64 {
    let (quantized_x, quantized_z, level) = quantized_key(cell);
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

// Pre-inserted so Bevy's `calculate_bounds` (which only fills absent Aabbs)
// keeps the padded box; without the XZ pad, swaying tips could be culled at
// frustum edges.
fn grass_cell_aabb(cell: GrassCell, config: &GrassConfig) -> Aabb {
    let pad = GRID_CELL_SIZE / 2.0 + BLADE_MAX_OVERHANG + config.wind_strength * WIND_SWAY_FACTOR + AABB_BASE_PAD;
    Aabb::from_min_max(
        Vec3::new(cell.x - pad, cell.y, cell.z - pad),
        Vec3::new(cell.x + pad, cell.y + BLADE_HEIGHT_MAX, cell.z + pad),
    )
}

#[cfg(test)]
mod tests {
    use bevy::mesh::VertexAttributeValues;

    use super::*;

    fn test_cell() -> GrassCell {
        GrassCell {
            x: GRID_CELL_SIZE * 2.5,
            y: 0.0,
            z: -GRID_CELL_SIZE * 1.5,
            level: 0,
        }
    }

    const ALL_OPEN: OpenEdges = OpenEdges {
        pos_x: true,
        neg_x: true,
        pos_z: true,
        neg_z: true,
    };
    const ALL_CLOSED: OpenEdges = OpenEdges {
        pos_x: false,
        neg_x: false,
        pos_z: false,
        neg_z: false,
    };

    fn positions(mesh: &Mesh) -> &[[f32; 3]] {
        match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(values)) => values,
            _ => panic!("grass mesh positions missing or not Float32x3"),
        }
    }

    fn uvs(mesh: &Mesh) -> &[[f32; 2]] {
        match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
            Some(VertexAttributeValues::Float32x2(values)) => values,
            _ => panic!("grass mesh uvs missing or not Float32x2"),
        }
    }

    fn colors(mesh: &Mesh) -> &[[f32; 4]] {
        match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
            Some(VertexAttributeValues::Float32x4(values)) => values,
            _ => panic!("grass mesh colors missing or not Float32x4"),
        }
    }

    #[test]
    fn same_cell_produces_identical_mesh() {
        let config = GrassConfig::default();
        let first = grass_cell_mesh(test_cell(), &config, ALL_OPEN);
        let second = grass_cell_mesh(test_cell(), &config, ALL_OPEN);
        assert_eq!(positions(&first), positions(&second));
        assert_eq!(uvs(&first), uvs(&second));
        assert_eq!(colors(&first), colors(&second));
    }

    #[test]
    fn blade_count_scales_with_density() {
        let sparse = GrassConfig {
            tufts_per_m2: 2.0,
            ..GrassConfig::default()
        };
        let dense = GrassConfig {
            tufts_per_m2: 4.0,
            ..GrassConfig::default()
        };
        let sparse_mesh = grass_cell_mesh(test_cell(), &sparse, ALL_OPEN);
        let dense_mesh = grass_cell_mesh(test_cell(), &dense, ALL_OPEN);
        assert_eq!(
            positions(&sparse_mesh).len(),
            cell_tuft_count(&sparse) * BLADES_PER_TUFT * VERTICES_PER_BLADE
        );
        assert_eq!(
            positions(&dense_mesh).len(),
            cell_tuft_count(&dense) * BLADES_PER_TUFT * VERTICES_PER_BLADE
        );
        assert_eq!(
            sparse_mesh.indices().map_or(0, bevy::mesh::Indices::len),
            cell_tuft_count(&sparse) * BLADES_PER_TUFT * INDICES_PER_BLADE
        );
        assert!(positions(&dense_mesh).len() > positions(&sparse_mesh).len());
    }

    #[test]
    fn root_vertices_have_zero_sway_weight() {
        let cell = test_cell();
        let mesh = grass_cell_mesh(cell, &GrassConfig::default(), ALL_OPEN);
        for (position, uv) in positions(&mesh).iter().zip(uvs(&mesh)) {
            match uv[0] {
                0.0 => assert!((position[1] - cell.y).abs() < f32::EPSILON),
                MID_SWAY_WEIGHT | 1.0 => assert!(position[1] > cell.y),
                weight => panic!("grass sway weight {weight} is not a root, mid, or tip ring"),
            }
        }
    }

    #[test]
    fn blades_stay_within_cell_plus_overhang() {
        let cell = test_cell();
        let config = GrassConfig::default();
        let mesh = grass_cell_mesh(cell, &config, ALL_OPEN);
        let aabb = grass_cell_aabb(cell, &config);
        let bound = GRID_CELL_SIZE / 2.0 + BLADE_MAX_OVERHANG;
        let max_sway = config.wind_strength * WIND_SWAY_FACTOR;
        for position in positions(&mesh) {
            assert!((position[0] - cell.x).abs() <= bound);
            assert!((position[2] - cell.z).abs() <= bound);
            assert!(position[1] >= cell.y && position[1] <= cell.y + BLADE_HEIGHT_MAX);

            // The padded AABB must contain every vertex even at full sway.
            let swayed_min = Vec3::from_array(*position) - Vec3::new(max_sway, 0.0, max_sway);
            let swayed_max = Vec3::from_array(*position) + Vec3::new(max_sway, 0.0, max_sway);
            assert!(swayed_min.cmpge(aabb.min().into()).all());
            assert!(swayed_max.cmple(aabb.max().into()).all());
        }
    }

    #[test]
    fn closed_edges_keep_blades_inside_cell() {
        let cell = test_cell();
        let mesh = grass_cell_mesh(cell, &GrassConfig::default(), ALL_CLOSED);
        let bound = GRID_CELL_SIZE / 2.0;
        for position in positions(&mesh) {
            assert!((position[0] - cell.x).abs() <= bound);
            assert!((position[2] - cell.z).abs() <= bound);
        }
    }
}
