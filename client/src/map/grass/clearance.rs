use bevy::prelude::*;
use common::protocol::{CarrierId, Floor, MapLayout, Ramp, RampShape};

use super::{
    mesh::{BLADE_HEIGHT_MAX, BLADE_MAX_OVERHANG, WIND_SWAY_FACTOR},
    patch::GrassPatch,
};
use crate::constants::GRASS_WIND_STRENGTH;

#[derive(Default)]
pub(super) struct GrassClearance {
    floors: Vec<Floor>,
    ramps: Vec<Ramp>,
}

const PAD: f32 = BLADE_MAX_OVERHANG + GRASS_WIND_STRENGTH * WIND_SWAY_FACTOR;

impl GrassClearance {
    pub fn new(layout: &MapLayout, carrier: CarrierId) -> Self {
        Self {
            floors: layout
                .floors
                .iter()
                .filter(|floor| floor.carrier == carrier)
                .cloned()
                .collect(),
            ramps: layout
                .ramps
                .iter()
                .filter(|ramp| ramp.carrier == carrier)
                .cloned()
                .collect(),
        }
    }

    pub fn for_patches(&self, patches: &[GrassPatch]) -> Self {
        let overlaps = |(x1, x2, z1, z2): (f32, f32, f32, f32)| {
            patches
                .iter()
                .any(|p| p.x2 + PAD >= x1 && p.x1 - PAD <= x2 && p.z2 + PAD >= z1 && p.z1 - PAD <= z2)
        };
        Self {
            floors: self
                .floors
                .iter()
                .filter(|floor| overlaps(floor.bounds_xz()))
                .cloned()
                .collect(),
            ramps: self
                .ramps
                .iter()
                .filter(|ramp| overlaps(ramp.bounds_xz()))
                .cloned()
                .collect(),
        }
    }

    pub fn allows(&self, root: Vec3) -> bool {
        let footprint = |(x1, x2, z1, z2): (f32, f32, f32, f32)| {
            root.x + PAD >= x1 && root.x - PAD <= x2 && root.z + PAD >= z1 && root.z - PAD <= z2
        };
        // The support slab ends at the root; only geometry above it can cut
        // into a blade. Planks use the same vertical thickness as Ramp::prism.
        let intersects = |bottom: f32, top: f32| top > root.y + 0.001 && bottom < root.y + BLADE_HEIGHT_MAX;
        !self
            .floors
            .iter()
            .any(|floor| footprint(floor.bounds_xz()) && intersects(floor.y - floor.thickness, floor.y))
            && !self.ramps.iter().any(|ramp| {
                if !footprint(ramp.bounds_xz()) {
                    return false;
                }
                let heights = [-PAD, PAD]
                    .into_iter()
                    .flat_map(|dx| [-PAD, PAD].map(|dz| ramp.surface_at(root.x + dx, root.z + dz)));
                let (low, high) =
                    heights.fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), h| (lo.min(h), hi.max(h)));
                let bottom = match ramp.shape {
                    RampShape::Plank => low - ramp.thickness,
                    RampShape::Solid => ramp.y,
                };
                intersects(bottom, high)
            })
    }
}

#[cfg(test)]
#[path = "tests/clearance.rs"]
mod tests;
