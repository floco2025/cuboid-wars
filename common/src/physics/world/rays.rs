use bevy_math::Vec3;
use rapier3d::prelude::{Collider, ColliderHandle, Group, Ray};

use super::{
    CollisionWorld,
    colliders::{
        ColliderKind, WALL_COLLISION_GROUP, character_collision_groups, field_blocks, ground_collision_groups,
        query_filter, world_collision_groups,
    },
};
use crate::{
    math::{from_rapier, to_rapier},
    protocol::{CarrierId, FieldId},
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
    pub fn support_surface_on_carrier(
        &self,
        origin: Vec3,
        max_distance: f32,
        carrier: CarrierId,
        passable: &[FieldId],
    ) -> Option<WorldSurfaceHit> {
        if !origin.is_finite() || !max_distance.is_finite() || max_distance <= 0.0 {
            return None;
        }
        let ray = Ray::new(to_rapier(origin), to_rapier(Vec3::NEG_Y));
        let allow = |_: ColliderHandle, collider: &Collider| {
            ColliderKind::carrier_from_user_data(collider.user_data) == carrier && field_blocks(collider, passable)
        };
        let mut filter = query_filter(character_collision_groups());
        filter.predicate = Some(&allow);
        let (collider, hit) = self
            .query_pipeline(filter)
            .cast_ray_and_get_normal(&ray, max_distance, false)?;
        let normal = from_rapier(hit.normal);
        (normal.y > 0.1).then_some(WorldSurfaceHit {
            point: origin + Vec3::NEG_Y * hit.time_of_impact,
            normal,
            carrier,
            collider,
        })
    }

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

    // Every world surface the ray enters before `max_distance`, nearest
    // first, one per collider; a solid's far face is never among them. A
    // pressure plate is a fixture lying on its floor, not a surface between
    // two spaces, so it is not among them either.
    #[must_use]
    pub fn world_surfaces_along_ray(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Vec<WorldSurfaceHit> {
        if !origin.is_finite() || !direction.is_finite() || !max_distance.is_finite() || max_distance <= 0.0 {
            return Vec::new();
        }
        let Some(direction) = direction.try_normalize() else {
            return Vec::new();
        };
        let ray = Ray::new(to_rapier(origin), to_rapier(direction));
        let structural = |_: ColliderHandle, collider: &Collider| {
            ColliderKind::from_user_data(collider.user_data) != Some(ColliderKind::PressurePlate)
        };
        let mut filter = query_filter(world_collision_groups());
        filter.predicate = Some(&structural);
        let pipeline = self.query_pipeline(filter);
        let mut hits: Vec<(f32, WorldSurfaceHit)> = pipeline
            .intersect_ray(ray, max_distance, false)
            .filter_map(|(handle, _, hit)| {
                let normal = from_rapier(hit.normal).try_normalize()?;
                Some((
                    hit.time_of_impact,
                    WorldSurfaceHit {
                        point: origin + direction * hit.time_of_impact,
                        normal,
                        carrier: self.carrier_of(handle),
                        collider: handle,
                    },
                ))
            })
            .collect();
        hits.sort_by(|a, b| a.0.total_cmp(&b.0));
        hits.into_iter().map(|(_, hit)| hit).collect()
    }

    #[must_use]
    pub fn attack_path_clear(&self, from: Vec3, to: Vec3, open_fields: &[FieldId]) -> bool {
        let displacement = to - from;
        self.attack_surface_along_ray(from, displacement, displacement.length(), open_fields)
            .is_none()
    }

    #[must_use]
    pub fn attack_surface_along_ray(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        open_fields: &[FieldId],
    ) -> Option<WorldSurfaceHit> {
        self.surface_along_ray(
            origin,
            direction,
            max_distance,
            character_collision_groups(),
            open_fields,
        )
    }

    // Keys grant personal passage; only a field that is off opens a shot path.
    #[must_use]
    pub fn portal_surface_along_ray(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        open_fields: &[FieldId],
    ) -> Option<WorldSurfaceHit> {
        let hit = self.attack_surface_along_ray(origin, direction, max_distance, open_fields)?;
        if ColliderKind::field_from_user_data(self.colliders[hit.collider].user_data).is_some()
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
        passable: &[FieldId],
    ) -> Option<WorldSurfaceHit> {
        if !origin.is_finite() || !direction.is_finite() || !max_distance.is_finite() || max_distance <= 0.0 {
            return None;
        }
        let direction = direction.try_normalize()?;
        let ray = Ray::new(to_rapier(origin), to_rapier(direction));
        let allow = |_: ColliderHandle, collider: &Collider| field_blocks(collider, passable);
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
