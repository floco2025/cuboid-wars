use bevy_math::{Vec3, Vec3Swizzles};

use crate::{
    math::PHYSICS_EPSILON,
    protocol::{Ramp, RampDirection, RampShape},
};

// The slope's four corners: the low pair at `Ramp::y`, the high pair a rise
// above it, index `i` of both on the same side of the run.
#[derive(Debug, Clone, Copy)]
pub struct RampCorners {
    pub low: [Vec3; 2],
    pub high: [Vec3; 2],
}

// A ramp's volume: the cross-section along its run, swept across its width.
#[derive(Debug, Clone)]
pub struct RampPrism {
    pub profile: Vec<Vec3>,
    pub sweep: Vec3,
}

impl RampPrism {
    pub fn points(&self) -> impl Iterator<Item = Vec3> + '_ {
        self.profile.iter().flat_map(|&point| [point, point + self.sweep])
    }
}

impl Ramp {
    #[must_use]
    pub fn corners(&self) -> RampCorners {
        let (min_x, max_x, min_z, max_z) = self.bounds_xz();
        let [low, high] = match self.direction {
            RampDirection::North => [[(min_x, max_z), (max_x, max_z)], [(min_x, min_z), (max_x, min_z)]],
            RampDirection::South => [[(min_x, min_z), (max_x, min_z)], [(min_x, max_z), (max_x, max_z)]],
            RampDirection::East => [[(min_x, min_z), (min_x, max_z)], [(max_x, min_z), (max_x, max_z)]],
            RampDirection::West => [[(max_x, min_z), (max_x, max_z)], [(min_x, min_z), (min_x, max_z)]],
        };
        RampCorners {
            low: low.map(|(x, z)| Vec3::new(x, self.y, z)),
            high: high.map(|(x, z)| Vec3::new(x, self.y + self.height, z)),
        }
    }

    // The surface height over (x, z), clamped to the footprint along the run.
    #[must_use]
    pub fn surface_at(&self, x: f32, z: f32) -> f32 {
        let corners = self.corners();
        let run = (corners.high[0] - corners.low[0]).xz();
        let length_squared = run.length_squared();
        if length_squared < PHYSICS_EPSILON * PHYSICS_EPSILON {
            return self.y;
        }
        let progress = ((Vec3::new(x, 0.0, z) - corners.low[0]).xz().dot(run) / length_squared).clamp(0.0, 1.0);
        self.y + progress * self.height
    }

    // The slope's upward normal; none for a footprint without a run or a width.
    #[must_use]
    pub fn surface_normal(&self) -> Option<Vec3> {
        let corners = self.corners();
        let normal = (corners.low[1] - corners.low[0])
            .cross(corners.high[0] - corners.low[0])
            .try_normalize()?;
        Some(if normal.y < 0.0 { -normal } else { normal })
    }

    // A plank's underside is the slope moved straight down, so both of its end
    // faces are vertical and meet the side of an adjoining floor slab.
    #[must_use]
    pub fn prism(&self) -> RampPrism {
        let corners = self.corners();
        let (low, high) = (corners.low[0], corners.high[0]);
        let profile = match self.shape {
            RampShape::Solid => vec![low, high.with_y(self.y), high],
            RampShape::Plank => {
                let drop = Vec3::Y * self.thickness;
                vec![low, low - drop, high - drop, high]
            }
        };
        RampPrism {
            profile,
            sweep: corners.low[1] - corners.low[0],
        }
    }
}
