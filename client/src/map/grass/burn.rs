use super::{
    mesh::BLADE_MAX_OVERHANG,
    patch::GrassPatch,
    streaming::{GrassChunkVisual, grass_chunk_mesh},
};
use crate::{
    constants::EXPLOSION_GRASS_BURN_CORE_RADIUS_FACTOR,
    vfx::{ClipRegion, ScorchOutline},
};
use bevy::prelude::*;
use common::protocol::CarrierId;
use std::collections::HashMap;

// Well under a storey, so a burn never reaches the floor above, yet enough
// for a blast on the rolling grounds to reach the blades up and down the slope.
pub(super) const BURN_VERTICAL_TOLERANCE: f32 = 1.5;

// `center` is in the carrier's frame, like the grass it burns; `region` is
// where the scorch mark shows, in the mark's own plane.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct GrassBurn {
    pub(crate) carrier: CarrierId,
    center: Vec3,
    radius: f32,
    rotation: f32,
    outline: ScorchOutline,
    region: ClipRegion,
    intensity: f32,
}

impl GrassBurn {
    pub(crate) fn new(
        carrier: CarrierId,
        center: Vec3,
        radius: f32,
        rotation: f32,
        mesh_index: usize,
        region: ClipRegion,
    ) -> Self {
        Self {
            carrier,
            center,
            radius,
            rotation,
            outline: ScorchOutline::for_mesh(mesh_index),
            region,
            intensity: 1.0,
        }
    }

    pub(crate) fn set_intensity(&mut self, intensity: f32) {
        self.intensity = intensity.clamp(0.0, 1.0);
    }

    pub(crate) fn strength_at(&self, root: Vec3) -> f32 {
        if self.radius <= 0.0 || (root.y - self.center.y).abs() > BURN_VERTICAL_TOLERANCE {
            return 0.0;
        }

        let offset = Vec2::new(root.x - self.center.x, root.z - self.center.z);
        let distance = offset.length();
        let angle = offset.y.atan2(offset.x) + self.rotation;
        let outer_radius = self.radius * self.outline.radius_factor(angle);
        if distance >= outer_radius {
            return 0.0;
        }
        // The mark's own plane: its unit disc, spun by `rotation` (`ScorchPlacement::new`).
        let plane_point = Vec2::from_angle(self.rotation).rotate(offset) / (2.0 * self.radius);
        if !self.region.contains(plane_point) {
            return 0.0;
        }

        let inner_radius = outer_radius * EXPLOSION_GRASS_BURN_CORE_RADIUS_FACTOR;
        let edge = SmoothStepCurve.sample_clamped(f32::inverse_lerp(inner_radius, outer_radius, distance));
        (1.0 - edge) * self.intensity
    }

    fn intersects_patch(&self, patch: GrassPatch) -> bool {
        if patch.carrier != self.carrier || (self.center.y - patch.y).abs() > BURN_VERTICAL_TOLERANCE {
            return false;
        }
        let closest_x = self
            .center
            .x
            .clamp(patch.x1 - BLADE_MAX_OVERHANG, patch.x2 + BLADE_MAX_OVERHANG);
        let closest_z = self
            .center
            .z
            .clamp(patch.z1 - BLADE_MAX_OVERHANG, patch.z2 + BLADE_MAX_OVERHANG);
        Vec2::new(self.center.x - closest_x, self.center.z - closest_z).length_squared() <= self.radius * self.radius
    }
}

pub fn grass_burn_system(
    mut previous_burns: Local<HashMap<Entity, GrassBurn>>,
    burns: Query<(Entity, &GrassBurn)>,
    chunks: Query<(Ref<GrassChunkVisual>, &Mesh3d)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let current_burns: HashMap<Entity, GrassBurn> = burns.iter().map(|(entity, burn)| (entity, burn.clone())).collect();
    let mut dirty_footprints = Vec::new();

    for (entity, burn) in &current_burns {
        match previous_burns.get(entity) {
            Some(previous) if previous == burn => {}
            Some(previous) => dirty_footprints.extend([previous.clone(), burn.clone()]),
            None => dirty_footprints.push(burn.clone()),
        }
    }
    for (entity, burn) in previous_burns.iter() {
        if !current_burns.contains_key(entity) {
            dirty_footprints.push(burn.clone());
        }
    }

    for (visual, mesh_handle) in &chunks {
        let dirty = dirty_footprints
            .iter()
            .any(|burn| visual.patches.iter().any(|patch| burn.intersects_patch(*patch)));
        if !dirty && !visual.is_added() {
            continue;
        }

        let affecting_burns: Vec<GrassBurn> = current_burns
            .values()
            .filter(|burn| visual.patches.iter().any(|patch| burn.intersects_patch(*patch)))
            .cloned()
            .collect();
        if !dirty && affecting_burns.is_empty() {
            continue;
        }

        // Inserted rather than edited in place: a chunk mesh lives only in
        // the render world once uploaded, so there is nothing to edit.
        if let Some(rebuilt) = grass_chunk_mesh(&visual, &affecting_burns) {
            meshes
                .insert(mesh_handle.id(), rebuilt)
                .expect("grass chunk mesh handle is no longer valid");
        }
    }

    *previous_burns = current_burns;
}
