use super::mesh::{AABB_BASE_PAD, BLADE_HEIGHT_MAX, BLADE_MAX_OVERHANG, WIND_SWAY_FACTOR, grass_cell_mesh};
use crate::{
    carriers::{CarrierEntities, CarrierStoreys},
    config::{ClientSettings, GrassConfig},
    constants::{GRASS_WIND_DIRECTION_DEGREES, GRASS_WIND_SPEED, GRASS_WIND_STRENGTH},
    materials::{GrassMaterial, GrassWindExtension},
};
use bevy::{camera::primitives::Aabb, light::NotShadowCaster, prelude::*};
use common::protocol::{CarrierId, GrassCell, MapLayout, MapSettings};
use std::collections::HashSet;

#[derive(Component)]
pub struct GrassMarker;

#[derive(Component, Clone, Copy)]
pub struct GrassCellVisual {
    pub(super) cell: GrassCell,
    pub(super) open: OpenEdges,
}

// Which cell edges border another grass cell on the same level of the same
// carrier; scatter may reach (and slightly overhang) the border only on
// those edges.
#[derive(Clone, Copy)]
pub(super) struct OpenEdges {
    pub(super) pos_x: bool,
    pub(super) neg_x: bool,
    pub(super) pos_z: bool,
    pub(super) neg_z: bool,
}

impl OpenEdges {
    fn for_cell(cell: GrassCell, cell_size: f32, painted: &HashSet<GrassKey>) -> Self {
        let (carrier, x, z, level) = quantized_key(cell, cell_size);
        Self {
            pos_x: painted.contains(&(carrier, x + 2, z, level)),
            neg_x: painted.contains(&(carrier, x - 2, z, level)),
            pos_z: painted.contains(&(carrier, x, z + 2, level)),
            neg_z: painted.contains(&(carrier, x, z - 2, level)),
        }
    }
}

// Spawn one mesh entity per grass cell in the current `MapLayout`. Re-runs
// whenever `MapLayout` is inserted or replaced (e.g., reconnect / map change).
pub fn grass_spawn_system(
    mut commands: Commands,
    map_layout: Res<MapLayout>,
    map_settings: Res<MapSettings>,
    client_settings: Res<ClientSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GrassMaterial>>,
    carrier_entities: Res<CarrierEntities>,
    storeys: Res<CarrierStoreys>,
    existing: Query<Entity, With<GrassMarker>>,
) {
    let layout = map_layout;
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

    let cell_size = map_settings.geometry.grid_cell_size;
    let material = materials.add(grass_material(grass));
    let painted: HashSet<GrassKey> = layout.grass.iter().map(|c| quantized_key(*c, cell_size)).collect();

    for cell in layout.grass.iter().copied() {
        let open = OpenEdges::for_cell(cell, cell_size, &painted);
        commands.spawn((
            GrassMarker,
            GrassCellVisual { cell, open },
            storeys.tag(cell.carrier, cell.level, 0),
            ChildOf(carrier_entities.get(cell.carrier)),
            Mesh3d(meshes.add(grass_cell_mesh(cell, cell_size, grass, open, &[]))),
            MeshMaterial3d(material.clone()),
            Transform::default(),
            Visibility::Visible,
            // Belt-and-braces with `GrassWindExtension::enable_shadows()`.
            NotShadowCaster,
            grass_cell_aabb(cell, cell_size, grass),
        ));
    }
}

fn grass_material(_config: &GrassConfig) -> GrassMaterial {
    let wind_direction = Vec2::from_angle(GRASS_WIND_DIRECTION_DEGREES.to_radians());
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
                GRASS_WIND_STRENGTH,
                GRASS_WIND_SPEED,
            ),
        },
    }
}

// A grass cell's identity: its carrier, its quantized cell center, and its
// level. Cell centers sit at odd multiples of half a cell, so doubling
// before rounding recovers a stable integer independent of float noise —
// all clients render identical grass regardless of `Vec` ordering. Adjacent
// cells differ by exactly 2 in the quantized coordinate.
pub(super) type GrassKey = (CarrierId, i64, i64, u8);

pub(super) fn quantized_key(cell: GrassCell, cell_size: f32) -> GrassKey {
    let quantized_x = (cell.x * 2.0 / cell_size).round() as i64;
    let quantized_z = (cell.z * 2.0 / cell_size).round() as i64;
    (cell.carrier, quantized_x, quantized_z, cell.level)
}

// Pre-inserted so Bevy's `calculate_bounds` (which only fills absent Aabbs)
// keeps the padded box; without the XZ pad, swaying tips could be culled at
// frustum edges.
pub(super) fn grass_cell_aabb(cell: GrassCell, cell_size: f32, _config: &GrassConfig) -> Aabb {
    let pad = cell_size / 2.0 + BLADE_MAX_OVERHANG + GRASS_WIND_STRENGTH * WIND_SWAY_FACTOR + AABB_BASE_PAD;
    Aabb::from_min_max(
        Vec3::new(cell.x - pad, cell.y, cell.z - pad),
        Vec3::new(cell.x + pad, cell.y + BLADE_HEIGHT_MAX, cell.z + pad),
    )
}
