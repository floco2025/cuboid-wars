use crate::map::{ActorSpawnZone, CarrierGrid};
use bevy::prelude::{IVec3, Resource, Vec3};
use common::{
    config::CharacterPhysicsConfig,
    map::{CarrierPose, ZoneVolume},
    physics::CollisionWorld,
    protocol::{BarrierId, BridgeId, Position},
};
use rand::{Rng, RngExt};
use std::collections::HashMap;

#[derive(Resource, Default)]
pub(crate) struct AirHomes(pub HashMap<usize, AirHome>);

pub(crate) struct AirHome {
    volume: ZoneVolume,
    origin: Vec3,
    center_height: f32,
    pub spacing: f32,
    range: f32,
    pose: CarrierPose,
    sample: IVec3,
    sample_end: IVec3,
    complete: bool,
    points: Vec<Vec3>,
    open: Vec<BarrierId>,
    pub age: f32,
    pub powered: Vec<BridgeId>,
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
        let volume = zone.volume(grid);
        let center_height = physics.movement_collider.height / 2.0;
        let spacing = physics.movement_collider.radius();
        let min = volume.min - Vec3::splat(range);
        let max = volume.max + Vec3::splat(range);
        Self {
            volume,
            origin: min - Vec3::Y * center_height,
            center_height,
            spacing,
            range,
            pose,
            sample: IVec3::ZERO,
            sample_end: ((max - min) / spacing).ceil().as_ivec3(),
            complete: false,
            points: Vec::new(),
            open: open.to_vec(),
            age: 0.0,
            powered: Vec::new(),
            last_tick: None,
        }
    }

    pub fn stale(&self, pose: CarrierPose, open: &[BarrierId], moving: bool) -> bool {
        self.open != open || (self.ready() && self.age >= 1.0 && (moving || self.pose != pose))
    }

    pub fn ready(&self) -> bool {
        self.complete
    }

    pub fn follow_pose(&mut self, pose: CarrierPose) {
        self.pose = pose;
    }

    pub fn path_contains(&self, from: Vec3, to: Vec3, pose: CarrierPose) -> bool {
        self.contains(from, pose) && self.contains(to, pose)
    }

    pub fn advance(&mut self, world: &CollisionWorld, physics: CharacterPhysicsConfig, budget: &mut usize) {
        while !self.complete && *budget > 0 {
            *budget -= 1;
            let point = self.origin + self.sample.as_vec3() * self.spacing;
            let world_point = self.pose.transform_point(point);
            if self.contains(world_point, self.pose)
                && !world.character_overlaps_solid(&world_point.into(), physics, &self.open)
            {
                self.points.push(point);
            }
            self.sample.x += 1;
            if self.sample.x > self.sample_end.x {
                self.sample.x = 0;
                self.sample.z += 1;
            }
            if self.sample.z > self.sample_end.z {
                self.sample.z = 0;
                self.sample.y += 1;
            }
            if self.sample.y > self.sample_end.y {
                self.complete = true;
                self.age = 0.0;
            }
        }
    }

    pub fn contains(&self, world: Vec3, pose: CarrierPose) -> bool {
        self.volume.contains(
            pose.inverse_transform_point(world) + Vec3::Y * self.center_height,
            self.range,
        )
    }

    pub fn destination(&self, pose: CarrierPose, rng: &mut impl Rng) -> Option<Position> {
        if self.points.is_empty() {
            return None;
        }
        Some(
            pose.transform_point(self.points[rng.random_range(0..self.points.len())])
                .into(),
        )
    }

    pub fn nearest(&self, world: Vec3, pose: CarrierPose) -> Option<Position> {
        let local = pose.inverse_transform_point(world);
        self.points
            .iter()
            .min_by(|a, b| a.distance_squared(local).total_cmp(&b.distance_squared(local)))
            .map(|p| pose.transform_point(*p).into())
    }
}

#[cfg(test)]
#[path = "tests/home.rs"]
mod tests;
