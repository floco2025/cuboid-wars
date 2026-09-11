use bevy_math::Vec3;
use rapier3d::{
    parry::{query::ShapeCastOptions, shape::Ball},
    prelude::{Collider, ColliderHandle, Group, Pose},
};

use super::{
    CollisionWorld,
    colliders::{
        BARRIER_COLLISION_GROUP, BRIDGE_COLLISION_GROUP, ColliderKind, FLOOR_COLLISION_GROUP, WALL_COLLISION_GROUP,
        barrier_blocks, character_collision_groups, query_filter, surface_collision_groups, world_collision_groups,
    },
    shape_cast::ShapeCastHit,
};
use crate::{
    math::{from_rapier, to_rapier},
    protocol::BarrierId,
};

impl CollisionWorld {
    #[must_use]
    pub fn camera_arm_distance(&self, pivot: Vec3, offset: Vec3, radius: f32) -> f32 {
        if self.ball_overlaps_groups(pivot, radius, surface_collision_groups(), &[]) {
            return 0.0;
        }
        self.cast_moving_ball(pivot, offset, radius)
            .map_or(offset.length(), |hit| (offset.length() * hit.t - 0.01).max(0.0))
    }

    #[must_use]
    pub fn cast_moving_ball(&self, position: Vec3, translation: Vec3, radius: f32) -> Option<ShapeCastHit> {
        self.cast_moving_ball_with_filter(position, translation, radius, surface_collision_groups(), &[], &[])
    }

    #[must_use]
    pub fn cast_bouncing_ball_excluding(
        &self,
        position: Vec3,
        translation: Vec3,
        radius: f32,
        excluded_colliders: &[ColliderHandle],
    ) -> Option<ShapeCastHit> {
        self.cast_moving_ball_with_filter(
            position,
            translation,
            radius,
            world_collision_groups(),
            excluded_colliders,
            &[],
        )
    }

    #[must_use]
    pub fn projectile_path_clear(
        &self,
        position: Vec3,
        translation: Vec3,
        radius: f32,
        open_kinds: &[BarrierId],
    ) -> bool {
        !self.projectile_start_blocked(position, radius, open_kinds)
            && self.projectile_sweep_clear(position, translation, radius, open_kinds)
    }

    // The travel alone, ignoring where it starts: a body already touching
    // geometry still has to judge where it is going.
    #[must_use]
    pub fn projectile_sweep_clear(
        &self,
        position: Vec3,
        translation: Vec3,
        radius: f32,
        open_kinds: &[BarrierId],
    ) -> bool {
        let groups = character_collision_groups();
        self.cast_moving_ball_with_filter(position, translation, radius, groups, &[], open_kinds)
            .is_none()
    }

    #[must_use]
    pub fn projectile_start_blocked(&self, position: Vec3, radius: f32, open_kinds: &[BarrierId]) -> bool {
        let groups = character_collision_groups();
        self.ball_overlaps_groups(position, radius, groups, open_kinds)
    }

    #[must_use]
    pub fn cast_moving_ball_against_fields(
        &self,
        position: Vec3,
        translation: Vec3,
        radius: f32,
        open_kinds: &[BarrierId],
    ) -> Option<ShapeCastHit> {
        let groups = BARRIER_COLLISION_GROUP | BRIDGE_COLLISION_GROUP;
        self.cast_moving_ball_with_filter(position, translation, radius, groups, &[], open_kinds)
    }

    fn cast_moving_ball_with_filter(
        &self,
        position: Vec3,
        translation: Vec3,
        radius: f32,
        groups: Group,
        excluded_colliders: &[ColliderHandle],
        passable: &[BarrierId],
    ) -> Option<ShapeCastHit> {
        if translation.length_squared() == 0.0 {
            return None;
        }

        let allow = |handle: ColliderHandle, collider: &Collider| {
            !excluded_colliders.contains(&handle) && barrier_blocks(collider, passable)
        };
        let mut filter = query_filter(groups);
        filter.predicate = Some(&allow);
        let shape = Ball::new(radius);
        let pose = Pose::from_translation(to_rapier(position));
        let options = ShapeCastOptions {
            max_time_of_impact: 1.0,
            stop_at_penetration: false,
            ..ShapeCastOptions::default()
        };

        self.query_pipeline(filter)
            .cast_shape(&pose, to_rapier(translation), &shape, options)
            .map(|(handle, hit)| {
                let mut normal = from_rapier(hit.normal2);
                if normal.dot(translation) > 0.0 {
                    normal = -normal;
                }
                ShapeCastHit {
                    normal,
                    contact: from_rapier(hit.witness1),
                    t: hit.time_of_impact,
                    field_kind: ColliderKind::field_kind_from_user_data(self.colliders[handle].user_data),
                    carrier: self.carrier_of(handle),
                }
            })
    }

    // Transparent fields block attacks, but never awareness.
    #[must_use]
    pub fn line_of_sight_clear(&self, from: Vec3, to: Vec3) -> bool {
        const SIGHT_RADIUS: f32 = 0.08;
        let translation = to - from;
        self.cast_moving_ball_with_filter(from, translation, SIGHT_RADIUS, world_collision_groups(), &[], &[])
            .is_none()
    }

    #[must_use]
    pub fn projectile_spawn_overlaps_blocker(&self, position: Vec3, radius: f32, open_kinds: &[BarrierId]) -> bool {
        // Walls, floors, and powered bridges are always blockers. A barrier
        // blocks the muzzle unless it is currently open (pressure-plate
        // held) — an open barrier is gone visually and shots pass through
        // it, so the muzzle clipping it is fine.
        let groups = WALL_COLLISION_GROUP | FLOOR_COLLISION_GROUP | BRIDGE_COLLISION_GROUP | BARRIER_COLLISION_GROUP;
        self.ball_overlaps_groups(position, radius, groups, open_kinds)
    }

    fn ball_overlaps_groups(&self, position: Vec3, radius: f32, groups: Group, passable: &[BarrierId]) -> bool {
        let allow = |_: ColliderHandle, collider: &Collider| barrier_blocks(collider, passable);
        let mut filter = query_filter(groups);
        filter.predicate = Some(&allow);
        self.shape_overlaps(Pose::from_translation(to_rapier(position)), &Ball::new(radius), filter)
    }
}
