use bevy_math::Vec3;
use rapier3d::prelude::{Collider, ColliderHandle, Group, Ray};

use super::{
    CollisionWorld,
    colliders::{
        ColliderKind, WALL_COLLISION_GROUP, barrier_blocks, character_collision_groups, ground_collision_groups,
        query_filter, world_collision_groups,
    },
};
use crate::{
    math::{from_rapier, to_rapier},
    protocol::{BarrierId, CarrierId},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldSurfaceHit {
    pub point: Vec3,
    pub normal: Vec3,
    // Whose surface it is: what a portal placed here rides.
    pub carrier: CarrierId,
    pub(crate) collider: ColliderHandle,
}

impl CollisionWorld {
    #[must_use]
    pub fn ground_surface_below(&self, origin: Vec3, max_distance: f32) -> Option<WorldSurfaceHit> {
        let hit = self.surface_along_ray(origin, Vec3::NEG_Y, max_distance, ground_collision_groups(), &[])?;
        (hit.normal.y > 0.1).then_some(hit)
    }

    #[must_use]
    pub fn wall_surface_along_ray(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<WorldSurfaceHit> {
        let hit = self.surface_along_ray(origin, direction, max_distance, WALL_COLLISION_GROUP, &[])?;
        (hit.normal.y.abs() < 0.1).then_some(hit)
    }

    #[must_use]
    pub fn world_surface_along_ray(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<WorldSurfaceHit> {
        self.surface_along_ray(origin, direction, max_distance, world_collision_groups(), &[])
    }

    #[must_use]
    pub fn attack_path_clear(&self, from: Vec3, to: Vec3, open_barriers: &[BarrierId]) -> bool {
        let displacement = to - from;
        self.attack_surface_along_ray(from, displacement, displacement.length(), open_barriers)
            .is_none()
    }

    #[must_use]
    pub fn attack_surface_along_ray(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        open_barriers: &[BarrierId],
    ) -> Option<WorldSurfaceHit> {
        self.surface_along_ray(
            origin,
            direction,
            max_distance,
            character_collision_groups(),
            open_barriers,
        )
    }

    // Keys grant personal passage; only plate state opens a shot path.
    #[must_use]
    pub fn portal_surface_along_ray(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        open_barriers: &[BarrierId],
    ) -> Option<WorldSurfaceHit> {
        let hit = self.attack_surface_along_ray(origin, direction, max_distance, open_barriers)?;
        if ColliderKind::field_kind_from_user_data(self.colliders[hit.collider].user_data).is_some()
            || self.eraser_blocks_segment(origin, hit.point)
        {
            return None;
        }
        Some(hit)
    }

    fn surface_along_ray(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        groups: Group,
        passable: &[BarrierId],
    ) -> Option<WorldSurfaceHit> {
        if !origin.is_finite() || !direction.is_finite() || !max_distance.is_finite() || max_distance <= 0.0 {
            return None;
        }
        let direction = direction.try_normalize()?;
        let ray = Ray::new(to_rapier(origin), to_rapier(direction));
        let allow = |_: ColliderHandle, collider: &Collider| barrier_blocks(collider, passable);
        let mut filter = query_filter(groups);
        filter.predicate = Some(&allow);
        let (handle, hit) = self
            .query_pipeline(filter)
            .cast_ray_and_get_normal(&ray, max_distance, false)?;
        let normal = from_rapier(hit.normal).try_normalize()?;

        Some(WorldSurfaceHit {
            point: origin + direction * hit.time_of_impact,
            normal,
            carrier: self.carrier_of(handle),
            collider: handle,
        })
    }
}
