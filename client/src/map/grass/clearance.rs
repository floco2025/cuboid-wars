use bevy::prelude::*;
use common::{physics::Solid, protocol::CarrierId};

use super::{
    mesh::{BLADE_HEIGHT_MAX, BLADE_MAX_OVERHANG, WIND_SWAY_FACTOR},
    patch::GrassPatch,
};
use crate::constants::GRASS_WIND_STRENGTH;

// The structural solids on one carrier, which no blade may show through.
#[derive(Default)]
pub(super) struct GrassClearance {
    solids: Vec<Solid>,
}

const PAD: f32 = BLADE_MAX_OVERHANG + GRASS_WIND_STRENGTH * WIND_SWAY_FACTOR;

impl GrassClearance {
    pub fn new(solids: &[Solid], carrier: CarrierId) -> Self {
        Self {
            solids: solids
                .iter()
                .filter(|solid| solid.carrier == carrier)
                .cloned()
                .collect(),
        }
    }

    pub fn for_patches(&self, patches: &[GrassPatch]) -> Self {
        Self {
            solids: self
                .solids
                .iter()
                .filter(|solid| {
                    patches.iter().any(|p| {
                        p.x2 + PAD >= solid.min.x
                            && p.x1 - PAD <= solid.max.x
                            && p.z2 + PAD >= solid.min.z
                            && p.z1 - PAD <= solid.max.z
                    })
                })
                .cloned()
                .collect(),
        }
    }

    // A blade rooted inside a solid is gone, and so is one within a lean
    // and a sway of a top it could come up through: a slab's edge, a
    // plank's low end. Beside anything taller it only leans into a face,
    // out of sight. The slab a blade roots on ends at the root and is no
    // obstacle.
    pub fn allows(&self, root: Vec3) -> bool {
        let low = root + Vec3::Y * 0.001;
        let high = root + Vec3::Y * BLADE_HEIGHT_MAX;
        let pad = Vec3::new(PAD, 0.0, PAD);
        !self.solids.iter().any(|solid| {
            reaches(solid, low, high)
                || (tops_out_below(solid, low - pad, high + pad) && reaches(solid, low - pad, high + pad))
        })
    }
}

// Whether the solid reaches into the box: its bounds do, and none of its
// faces has the whole box in front of it. Exact for a box; a wedge may claim
// a box its edges just miss.
fn reaches(solid: &Solid, low: Vec3, high: Vec3) -> bool {
    solid.min.cmple(high).all()
        && solid.max.cmpge(low).all()
        && solid.faces.iter().all(|face| {
            let nearest = Vec3::select(face.normal.cmpgt(Vec3::ZERO), low, high);
            face.normal.dot(nearest - face.corners[0]) < 0.0
        })
}

// Whether a face of the solid that looks up lies below the box's top
// somewhere over the box's footprint.
fn tops_out_below(solid: &Solid, low: Vec3, high: Vec3) -> bool {
    solid.faces.iter().filter(|face| face.normal.y > 0.1).any(|face| {
        let normal = face.normal;
        let corner = face.corners[0];
        // The plane is lowest where the footprint reaches furthest down its slope.
        let x = if normal.x > 0.0 { high.x } else { low.x };
        let z = if normal.z > 0.0 { high.z } else { low.z };
        corner.y - (normal.x * (x - corner.x) + normal.z * (z - corner.z)) / normal.y < high.y
    })
}

#[cfg(test)]
#[path = "tests/clearance.rs"]
mod tests;
