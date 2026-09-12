use std::collections::HashMap;

use bevy::{
    math::DVec3,
    prelude::{Resource, Vec3},
};
use common::{
    config::CharacterPhysicsConfig,
    map::CarrierPose,
    physics::CollisionWorld,
    protocol::{BarrierId, Position},
};
use rand::{Rng, RngExt};

use crate::{
    actors::navigation::ActorTerritory,
    map::{ActorSpawnZone, CarrierGrid},
};

const HOME_SAMPLE_LIMIT: usize = 4096;
const HOME_REFRESH_SECS: f32 = 1.0;

#[derive(Resource, Default)]
pub(crate) struct AirHomes(pub HashMap<usize, AirHome>);

pub(crate) struct AirHome {
    territory: ActorTerritory,
    pub spacing: f32,
    pose: CarrierPose,
    build_pose: CarrierPose,
    revision: u64,
    sample: usize,
    points: Vec<Option<Vec3>>,
    open: Vec<BarrierId>,
    build_open: Vec<BarrierId>,
    pub age: f32,
    pub last_tick: Option<u32>,
}

impl AirHome {
    pub fn new(
        zone: &ActorSpawnZone,
        grid: &CarrierGrid,
        physics: CharacterPhysicsConfig,
        range: f32,
        pose: CarrierPose,
        open: &[BarrierId],
    ) -> Self {
        let territory = ActorTerritory {
            carrier: zone.carrier,
            volume: zone.volume(grid),
            distance: range,
            center_height: physics.movement_collider.height / 2.0,
        };
        let spacing = physics.movement_collider.radius();
        let size =
            territory.volume.max.as_dvec3() - territory.volume.min.as_dvec3() + DVec3::splat(f64::from(range) * 2.0);
        let samples =
            (size.element_product() / f64::from(spacing).powi(3)).clamp(32.0, HOME_SAMPLE_LIMIT as f64) as usize;
        Self {
            territory,
            spacing,
            pose,
            build_pose: pose,
            revision: 0,
            sample: 0,
            points: vec![None; samples],
            open: open.to_vec(),
            build_open: open.to_vec(),
            age: 0.0,
            last_tick: None,
        }
    }

    pub fn refresh(&mut self, world: &CollisionWorld, pose: CarrierPose, open: &[BarrierId]) {
        let padding = Vec3::splat(self.territory.distance + self.spacing * 2.0);
        let min = pose.transform_point(self.territory.volume.min) - padding;
        let max = pose.transform_point(self.territory.volume.max) + padding;
        let revision = world.region_revision(min, max);
        if self.ready()
            && (self.build_open != open
                || (self.age >= HOME_REFRESH_SECS && (self.build_pose != pose || self.revision != revision)))
        {
            // Keep samples usable while their replacements are checked against the current geometry.
            self.sample = 0;
            self.build_pose = pose;
            self.revision = revision;
        }
        if self.sample == 0 {
            self.build_pose = pose;
            self.revision = revision;
            self.build_open.clear();
            self.build_open.extend_from_slice(open);
        }
        self.open.clear();
        self.open.extend_from_slice(open);
        self.pose = pose;
    }

    pub fn ready(&self) -> bool {
        self.sample == self.points.len()
    }

    pub fn path_contains(&self, from: Vec3, to: Vec3, pose: CarrierPose) -> bool {
        self.territory
            .path_contains(pose.inverse_transform_point(from), pose.inverse_transform_point(to))
    }

    pub fn advance(&mut self, world: &CollisionWorld, physics: CharacterPhysicsConfig, budget: &mut usize) {
        let padding = DVec3::splat(f64::from(self.territory.distance));
        let min = self.territory.volume.min.as_dvec3() - padding;
        let max = self.territory.volume.max.as_dvec3() + padding;
        while !self.ready() && *budget > 0 {
            *budget -= 1;
            // Low-discrepancy samples cover all axes early instead of filling one corner first.
            let fraction = Vec3::new(
                radical_inverse(self.sample + 1, 2),
                radical_inverse(self.sample + 1, 3),
                radical_inverse(self.sample + 1, 5),
            );
            let point = (min + (max - min) * fraction.as_dvec3()).as_vec3() - Vec3::Y * self.territory.center_height;
            let world_point = self.pose.transform_point(point);
            self.points[self.sample] = (self.territory.contains_position(point)
                && !world.character_overlaps_solid(&world_point.into(), physics, &self.open))
            .then_some(point);
            self.sample += 1;
            if self.ready() {
                self.age = 0.0;
            }
        }
    }

    pub fn contains(&self, world: Vec3, pose: CarrierPose) -> bool {
        self.territory.contains_position(pose.inverse_transform_point(world))
    }

    pub fn destination(
        &self,
        pose: CarrierPose,
        world: &CollisionWorld,
        physics: CharacterPhysicsConfig,
        rng: &mut impl Rng,
    ) -> Option<Position> {
        let candidates: Vec<_> = self.points.iter().flatten().copied().collect();
        if candidates.is_empty() {
            return None;
        }
        (0..8).find_map(|_| {
            let point = pose
                .transform_point(candidates[rng.random_range(0..candidates.len())])
                .into();
            (!world.character_overlaps_solid(&point, physics, &self.open)).then_some(point)
        })
    }

    pub fn nearest(
        &self,
        position: Vec3,
        pose: CarrierPose,
        world: &CollisionWorld,
        physics: CharacterPhysicsConfig,
    ) -> Option<Position> {
        let local = pose.inverse_transform_point(position);
        let mut candidates: Vec<_> = self.points.iter().flatten().copied().collect();
        candidates.sort_unstable_by(|a, b| a.distance_squared(local).total_cmp(&b.distance_squared(local)));
        candidates.into_iter().take(8).find_map(|p| {
            let point = pose.transform_point(p).into();
            (!world.character_overlaps_solid(&point, physics, &self.open)).then_some(point)
        })
    }
}

fn radical_inverse(mut index: usize, base: usize) -> f32 {
    let mut value = 0.0;
    let mut fraction = 1.0 / base as f32;
    while index > 0 {
        value += (index % base) as f32 * fraction;
        index /= base;
        fraction /= base as f32;
    }
    value
}

#[cfg(test)]
#[path = "tests/home.rs"]
mod tests;
