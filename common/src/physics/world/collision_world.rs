use bevy_ecs::prelude::Resource;
use bevy_math::Vec3;
use rapier3d::{
    control::{CharacterCollision, EffectiveCharacterMovement, KinematicCharacterController},
    parry::{
        query::ShapeCastOptions,
        shape::{Ball, Cuboid},
    },
    prelude::{
        BroadPhaseBvh, ColliderSet, Group, IntegrationParameters, NarrowPhase, Pose, RigidBodySet, Shape, Vector,
    },
};

use crate::protocol::{BarrierKindId, BarrierKindTable, MapLayout};

use super::colliders::{
    FLOOR_COLLISION_GROUP, WALL_COLLISION_GROUP, barrier_collision_group, character_collision_groups,
    insert_barrier_collider, insert_floor_collider, insert_ramp_collider, insert_wall_collider, query_filter,
    world_collision_groups,
};

#[cfg(test)]
use super::colliders::ColliderKind;
pub(crate) use super::shape_cast::ShapeCastHit;
use super::shape_cast::upward_surface_hit;

#[derive(Resource)]
pub struct CollisionWorld {
    bodies: RigidBodySet,
    pub(super) colliders: ColliderSet,
    broad_phase: BroadPhaseBvh,
    narrow_phase: NarrowPhase,
    // Cached union of every configured barrier kind's collision group.
    // Recomputed at construction (depends on `BarrierKindTable.len()`); used
    // by character filters and barrier-only shape casts so we don't loop
    // the table per query.
    all_barrier_groups: Group,
}

impl CollisionWorld {
    #[must_use]
    pub fn from_map_layout(map_layout: &MapLayout, kind_table: &BarrierKindTable) -> Self {
        let bodies = RigidBodySet::new();
        let mut colliders = ColliderSet::new();
        let mut collider_handles = Vec::new();

        for wall in &map_layout.walls {
            collider_handles.push(insert_wall_collider(&mut colliders, wall));
        }

        for floor in &map_layout.floors {
            collider_handles.push(insert_floor_collider(&mut colliders, floor));
        }

        for ramp in &map_layout.ramps {
            if let Some(handle) = insert_ramp_collider(&mut colliders, ramp) {
                collider_handles.push(handle);
            }
        }

        for barrier in &map_layout.barriers {
            collider_handles.push(insert_barrier_collider(&mut colliders, barrier));
        }

        let mut broad_phase = BroadPhaseBvh::new();
        let narrow_phase = NarrowPhase::new();
        let mut events = Vec::new();
        broad_phase.update(
            &IntegrationParameters::default(),
            &colliders,
            &bodies,
            &collider_handles,
            &[],
            &mut events,
        );

        let mut all_barrier_groups = Group::empty();
        for idx in 0..kind_table.len() {
            all_barrier_groups |= barrier_collision_group(BarrierKindId(idx as u16));
        }

        Self {
            bodies,
            colliders,
            broad_phase,
            narrow_phase,
            all_barrier_groups,
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(super) fn solid_count(&self) -> usize {
        self.colliders.len()
    }

    #[cfg(test)]
    #[must_use]
    pub(super) fn solid_kinds(&self) -> Vec<ColliderKind> {
        self.colliders
            .iter()
            .filter_map(|(_, collider)| ColliderKind::from_user_data(collider.user_data))
            .collect()
    }

    #[must_use]
    pub(crate) fn move_character(
        &self,
        dt: f32,
        controller: &KinematicCharacterController,
        character_shape: &dyn Shape,
        character_pos: &Pose,
        desired_translation: Vector,
        has_phasing: bool,
        held_keys: &[BarrierKindId],
        events: impl FnMut(CharacterCollision),
    ) -> EffectiveCharacterMovement {
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            query_filter(character_collision_groups(
                has_phasing,
                held_keys,
                self.all_barrier_groups,
            )),
        );
        controller.move_shape(
            dt,
            &query_pipeline,
            character_shape,
            character_pos,
            desired_translation,
            events,
        )
    }

    // Cast a moving ball against walls/floors/ramps (the "bouncy" world).
    // Barriers terminate projectiles via `cast_moving_ball_against_barriers`
    // instead, so they're filtered out here.
    #[must_use]
    pub(crate) fn cast_moving_ball(&self, position: Vec3, translation: Vec3, radius: f32) -> Option<ShapeCastHit> {
        self.cast_moving_ball_with_filter(position, translation, radius, world_collision_groups())
    }

    // Cast a moving ball against barrier colliders only. Used by projectiles
    // to detect termination on a barrier.
    #[must_use]
    pub(crate) fn cast_moving_ball_against_barriers(
        &self,
        position: Vec3,
        translation: Vec3,
        radius: f32,
    ) -> Option<ShapeCastHit> {
        if self.all_barrier_groups.is_empty() {
            return None;
        }
        self.cast_moving_ball_with_filter(position, translation, radius, self.all_barrier_groups)
    }

    fn cast_moving_ball_with_filter(
        &self,
        position: Vec3,
        translation: Vec3,
        radius: f32,
        groups: Group,
    ) -> Option<ShapeCastHit> {
        if translation.length_squared() == 0.0 {
            return None;
        }

        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            query_filter(groups),
        );
        let shape = Ball::new(radius);
        let pose = Pose::translation(position.x, position.y, position.z);
        let velocity = Vector::new(translation.x, translation.y, translation.z);
        let options = ShapeCastOptions {
            max_time_of_impact: 1.0,
            stop_at_penetration: false,
            ..ShapeCastOptions::default()
        };

        query_pipeline
            .cast_shape(&pose, velocity, &shape, options)
            .map(|(_, hit)| ShapeCastHit {
                normal: Vec3::new(hit.normal2.x, hit.normal2.y, hit.normal2.z),
                t: hit.time_of_impact,
            })
    }

    // Line of sight is blocked by walls/floors/ramps AND by barriers (an
    // actor can't see — and therefore can't path-find — through a barrier
    // it can't pass through).
    #[must_use]
    pub fn line_of_sight_clear(&self, from: Vec3, to: Vec3) -> bool {
        const SIGHT_RADIUS: f32 = 0.08;
        let translation = to - from;
        self.cast_moving_ball(from, translation, SIGHT_RADIUS).is_none()
            && self
                .cast_moving_ball_against_barriers(from, translation, SIGHT_RADIUS)
                .is_none()
    }

    #[must_use]
    pub(crate) fn ground_hit(
        &self,
        character_shape: &dyn Shape,
        character_pos: &Pose,
        max_distance: f32,
        target_distance: f32,
        has_phasing: bool,
        held_keys: &[BarrierKindId],
    ) -> Option<ShapeCastHit> {
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            query_filter(character_collision_groups(
                has_phasing,
                held_keys,
                self.all_barrier_groups,
            )),
        );
        let options = ShapeCastOptions {
            max_time_of_impact: max_distance,
            target_distance,
            stop_at_penetration: false,
            compute_impact_geometry_on_penetration: true,
        };

        query_pipeline
            .cast_shape(character_pos, Vector::NEG_Y, character_shape, options)
            .and_then(|(_, hit)| upward_surface_hit(hit))
    }

    #[must_use]
    pub(crate) fn projectile_spawn_overlaps_blocker(&self, position: Vec3, radius: f32) -> bool {
        self.ball_overlaps_groups(position, radius, WALL_COLLISION_GROUP | FLOOR_COLLISION_GROUP)
    }

    #[must_use]
    pub fn cuboid_overlaps_wall(&self, position: Vec3, half_extents: Vec3) -> bool {
        self.cuboid_overlaps_groups(position, half_extents, WALL_COLLISION_GROUP)
    }

    fn ball_overlaps_groups(&self, position: Vec3, radius: f32, groups: Group) -> bool {
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            query_filter(groups),
        );
        let shape = Ball::new(radius);
        let pose = Pose::translation(position.x, position.y, position.z);

        query_pipeline.intersect_shape(pose, &shape).next().is_some()
    }

    fn cuboid_overlaps_groups(&self, center: Vec3, half_extents: Vec3, groups: Group) -> bool {
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            query_filter(groups),
        );
        let shape = Cuboid::new(Vector::new(half_extents.x, half_extents.y, half_extents.z));
        let pose = Pose::translation(center.x, center.y, center.z);

        query_pipeline.intersect_shape(pose, &shape).next().is_some()
    }
}
