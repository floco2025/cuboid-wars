use bevy_math::Vec3;
use rapier3d::{
    parry::{
        bounding_volume::Aabb,
        query::{Ray, RayCast, ShapeCastOptions, cast_shapes, intersection_test},
    },
    prelude::{Cuboid, Pose, Vector},
};

use super::CollisionWorld;
use crate::{
    config::CharacterPhysicsConfig,
    map::{CarrierPose, Carriers},
    math::to_rapier,
    physics::characters::{character_movement_pose, character_movement_shape},
    protocol::{CarrierId, Eraser, Position},
};

#[derive(Clone, Copy)]
pub(super) struct EraserVolume {
    min: Vec3,
    max: Vec3,
    carrier: CarrierId,
}

impl EraserVolume {
    pub(super) fn from_eraser(eraser: &Eraser) -> Self {
        let pad = eraser.width / 2.0;
        Self {
            min: Vec3::new(eraser.x1.min(eraser.x2) - pad, eraser.y, eraser.z1.min(eraser.z2) - pad),
            max: Vec3::new(
                eraser.x1.max(eraser.x2) + pad,
                eraser.y + eraser.height,
                eraser.z1.max(eraser.z2) + pad,
            ),
            carrier: eraser.carrier,
        }
    }

    // The volume built from a carrier-local eraser, placed by the carrier's pose.
    pub(super) fn posed(&self, pose: &CarrierPose) -> Self {
        Self {
            min: pose.transform_point(self.min),
            max: pose.transform_point(self.max),
            ..*self
        }
    }

    pub(super) const fn carrier(&self) -> CarrierId {
        self.carrier
    }
}

impl CollisionWorld {
    // Indices name the erasers in the layout for this collision world's lifetime.
    pub fn character_eraser_contacts<'a>(
        &'a self,
        start: &Position,
        end: &Position,
        physics: CharacterPhysicsConfig,
        carriers: Option<&'a Carriers>,
    ) -> impl Iterator<Item = usize> + 'a {
        let shape = character_movement_shape(physics);
        let pose = character_movement_pose(start, physics);
        let translation = Vec3::from(*end) - Vec3::from(*start);
        self.eraser_volumes
            .iter()
            .enumerate()
            .filter_map(move |(index, volume)| {
                // The field is posed at tick end; relative travel catches a moving field sweeping a stationary player.
                let carry = carriers.map_or(Vec3::ZERO, |carriers| carriers.displacement(volume.carrier));
                let field = Cuboid::new(to_rapier((volume.max - volume.min) / 2.0));
                let field_pose = Pose::from_translation(to_rapier((volume.min + volume.max) / 2.0));
                let mut from = pose;
                from.translation += to_rapier(carry);
                let overlaps = intersection_test(&from, &shape, &field_pose, &field).is_ok_and(|hit| hit);
                (overlaps
                    || cast_shapes(
                        &from,
                        to_rapier(translation - carry),
                        &shape,
                        &field_pose,
                        Vector::ZERO,
                        &field,
                        ShapeCastOptions {
                            max_time_of_impact: 1.0,
                            ..Default::default()
                        },
                    )
                    .is_ok_and(|hit| hit.is_some()))
                .then_some(index)
            })
    }

    pub(super) fn eraser_blocks_segment(&self, from: Vec3, to: Vec3) -> bool {
        let ray = Ray::new(to_rapier(from), to_rapier(to - from));
        self.eraser_volumes.iter().any(|volume| {
            Aabb::new(to_rapier(volume.min), to_rapier(volume.max))
                .cast_local_ray(&ray, 1.0, true)
                .is_some()
        })
    }
}

#[cfg(test)]
#[path = "tests/erasers.rs"]
mod tests;
