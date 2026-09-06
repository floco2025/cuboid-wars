use super::{
    mesh::{BLADE_MAX_OVERHANG, grass_cell_mesh},
    spawn::GrassCellVisual,
};
use crate::{config::ClientSettings, constants::EXPLOSION_GRASS_BURN_VERTICAL_TOLERANCE, vfx::ScorchOutline};
use bevy::prelude::*;
use common::protocol::{CarrierId, GrassCell, MapSettings};
use std::collections::HashMap;

// `center` is in the carrier's frame, like the grass it burns.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct GrassBurn {
    pub(crate) carrier: CarrierId,
    pub(super) center: Vec3,
    pub(super) radius: f32,
    pub(super) rotation: f32,
    pub(super) outline: ScorchOutline,
    pub(super) intensity: f32,
}

impl GrassBurn {
    pub(crate) fn new(carrier: CarrierId, center: Vec3, radius: f32, rotation: f32, mesh_index: usize) -> Self {
        Self {
            carrier,
            center,
            radius,
            rotation,
            outline: ScorchOutline::for_mesh(mesh_index),
            intensity: 1.0,
        }
    }

    pub(crate) fn set_intensity(&mut self, intensity: f32) {
        self.intensity = intensity.clamp(0.0, 1.0);
    }

    fn intersects_cell(self, cell: GrassCell, cell_size: f32) -> bool {
        if cell.carrier != self.carrier || (self.center.y - cell.y).abs() > EXPLOSION_GRASS_BURN_VERTICAL_TOLERANCE {
            return false;
        }
        let half_extent = cell_size * 0.5 + BLADE_MAX_OVERHANG;
        let closest_x = self.center.x.clamp(cell.x - half_extent, cell.x + half_extent);
        let closest_z = self.center.z.clamp(cell.z - half_extent, cell.z + half_extent);
        Vec2::new(self.center.x - closest_x, self.center.z - closest_z).length_squared() <= self.radius * self.radius
    }
}

pub fn grass_burn_system(
    mut previous_burns: Local<HashMap<Entity, GrassBurn>>,
    burns: Query<(Entity, &GrassBurn)>,
    cells: Query<(Ref<GrassCellVisual>, &Mesh3d)>,
    client_settings: Res<ClientSettings>,
    map_settings: Res<MapSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let cell_size = map_settings.geometry.grid_cell_size;
    let current_burns: HashMap<Entity, GrassBurn> = burns.iter().map(|(entity, burn)| (entity, *burn)).collect();
    let mut dirty_footprints = Vec::new();

    for (entity, burn) in &current_burns {
        match previous_burns.get(entity) {
            Some(previous) if previous == burn => {}
            Some(previous) => dirty_footprints.extend([*previous, *burn]),
            None => dirty_footprints.push(*burn),
        }
    }
    for (entity, burn) in previous_burns.iter() {
        if !current_burns.contains_key(entity) {
            dirty_footprints.push(*burn);
        }
    }

    for (visual, mesh_handle) in &cells {
        let dirty = dirty_footprints
            .iter()
            .any(|burn| burn.intersects_cell(visual.cell, cell_size));
        if !dirty && !visual.is_added() {
            continue;
        }

        let affecting_burns: Vec<GrassBurn> = current_burns
            .values()
            .copied()
            .filter(|burn| burn.intersects_cell(visual.cell, cell_size))
            .collect();
        if !dirty && affecting_burns.is_empty() {
            continue;
        }

        if let Some(mut mesh) = meshes.get_mut(&mesh_handle.0) {
            *mesh = grass_cell_mesh(
                visual.cell,
                cell_size,
                &client_settings.grass,
                visual.open,
                &affecting_burns,
            );
        }
    }

    *previous_burns = current_burns;
}
