use super::mesh::{AABB_BASE_PAD, BLADE_HEIGHT_MAX, BLADE_MAX_OVERHANG, WIND_SWAY_FACTOR, grass_cell_mesh};
use crate::{
    config::{ClientSettings, GrassConfig},
    map::MapLevel,
    materials::{GrassMaterial, GrassWindExtension},
};
use bevy::{camera::primitives::Aabb, light::NotShadowCaster, prelude::*};
use common::{
    constants::GRID_CELL_SIZE,
    protocol::{GrassCell, MapLayout},
};
use std::collections::HashSet;

#[derive(Component)]
pub struct GrassMarker;

#[derive(Component, Clone, Copy)]
pub struct GrassCellVisual {
    pub(super) cell: GrassCell,
    pub(super) open: OpenEdges,
}

// Which cell edges border another grass cell on the same level; scatter may
// reach (and slightly overhang) the border only on those edges.
#[derive(Clone, Copy)]
pub(super) struct OpenEdges {
    pub(super) pos_x: bool,
    pub(super) neg_x: bool,
    pub(super) pos_z: bool,
    pub(super) neg_z: bool,
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
            GrassCellVisual { cell, open },
            MapLevel(cell.level),
            Mesh3d(meshes.add(grass_cell_mesh(cell, grass, open, &[]))),
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

// Cell centers sit at odd multiples of `GRID_CELL_SIZE / 2`, so doubling
// before rounding recovers a stable integer independent of float noise —
// all clients render identical grass regardless of `Vec` ordering. Adjacent
// cells differ by exactly 2 in the quantized coordinate.
pub(super) fn quantized_key(cell: GrassCell) -> (i64, i64, u8) {
    let quantized_x = (cell.x * 2.0 / GRID_CELL_SIZE).round() as i64;
    let quantized_z = (cell.z * 2.0 / GRID_CELL_SIZE).round() as i64;
    (quantized_x, quantized_z, cell.level)
}

// Pre-inserted so Bevy's `calculate_bounds` (which only fills absent Aabbs)
// keeps the padded box; without the XZ pad, swaying tips could be culled at
// frustum edges.
pub(super) fn grass_cell_aabb(cell: GrassCell, config: &GrassConfig) -> Aabb {
    let pad = GRID_CELL_SIZE / 2.0 + BLADE_MAX_OVERHANG + config.wind_strength * WIND_SWAY_FACTOR + AABB_BASE_PAD;
    Aabb::from_min_max(
        Vec3::new(cell.x - pad, cell.y, cell.z - pad),
        Vec3::new(cell.x + pad, cell.y + BLADE_HEIGHT_MAX, cell.z + pad),
    )
}
