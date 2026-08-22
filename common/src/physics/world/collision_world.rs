use bevy_ecs::prelude::Resource;
use bevy_math::Vec3;
use rapier3d::{
    control::{CharacterCollision, EffectiveCharacterMovement, KinematicCharacterController},
    parry::{
        query::ShapeCastOptions,
        shape::{Ball, Cuboid},
    },
    prelude::{
        BroadPhaseBvh, ColliderSet, Group, IntegrationParameters, NarrowPhase, Pose, Ray, RigidBodySet, Shape, Vector,
    },
};

use crate::{
    config::CharacterPhysicsConfig,
    physics::characters::{character_center, character_shape},
    protocol::{BarrierKindId, BarrierKindTable, MapLayout, Position},
};

use super::colliders::{
    ColliderKind, FLOOR_COLLISION_GROUP, WALL_COLLISION_GROUP, barrier_collision_group, character_collision_groups,
    ground_collision_groups, insert_barrier_collider, insert_floor_collider, insert_ramp_collider,
    insert_wall_collider, query_filter, world_collision_groups,
};

use super::ladders::LadderVolume;
pub use super::shape_cast::ShapeCastHit;
use super::shape_cast::upward_surface_hit;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldSurfaceHit {
    pub point: Vec3,
    pub normal: Vec3,
}

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
    ladder_volumes: Vec<LadderVolume>,
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

        let ladder_volumes = map_layout.ladders.iter().map(LadderVolume::from_ladder).collect();

        Self {
            bodies,
            colliders,
            broad_phase,
            narrow_phase,
            all_barrier_groups,
            ladder_volumes,
        }
    }

    #[must_use]
    pub fn ladder_volume_at(&self, pos: &Position) -> Option<&LadderVolume> {
        self.ladder_volumes.iter().find(|volume| volume.contains(pos))
    }

    #[must_use]
    pub fn ladder_band_at(&self, x: f32, z: f32, y: f32) -> Option<&LadderVolume> {
        self.ladder_volumes.iter().find(|volume| volume.band_contains(x, z, y))
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
        passable_kinds: &[BarrierKindId],
        events: impl FnMut(CharacterCollision),
    ) -> EffectiveCharacterMovement {
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            query_filter(character_collision_groups(passable_kinds, self.all_barrier_groups)),
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
    pub fn cast_moving_ball(&self, position: Vec3, translation: Vec3, radius: f32) -> Option<ShapeCastHit> {
        self.cast_moving_ball_with_filter(position, translation, radius, world_collision_groups())
    }

    // Cast a moving ball against barrier colliders only. Used by projectiles
    // to detect termination on a barrier. Kinds in `open_kinds` (currently
    // held open by pressure plates) are dropped from the filter, so shots
    // fly through the gap a plate creates.
    #[must_use]
    pub fn cast_moving_ball_against_barriers(
        &self,
        position: Vec3,
        translation: Vec3,
        radius: f32,
        open_kinds: &[BarrierKindId],
    ) -> Option<ShapeCastHit> {
        let mut groups = self.all_barrier_groups;
        for kind in open_kinds {
            groups.remove(barrier_collision_group(*kind));
        }
        if groups.is_empty() {
            return None;
        }
        self.cast_moving_ball_with_filter(position, translation, radius, groups)
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
            .map(|(handle, hit)| {
                let mut normal = Vec3::new(hit.normal2.x, hit.normal2.y, hit.normal2.z);
                if normal.dot(translation) > 0.0 {
                    normal = -normal;
                }
                ShapeCastHit {
                    normal,
                    contact: Vec3::new(hit.witness1.x, hit.witness1.y, hit.witness1.z),
                    t: hit.time_of_impact,
                    barrier_kind: ColliderKind::barrier_kind_from_user_data(self.colliders[handle].user_data),
                }
            })
    }

    // Line of sight is blocked by walls/floors/ramps only. Barriers don't
    // block sight — actors see through and pursue; the kinematic controller
    // stops them at the barrier surface, where normal wall-avoidance kicks in.
    #[must_use]
    pub fn line_of_sight_clear(&self, from: Vec3, to: Vec3) -> bool {
        const SIGHT_RADIUS: f32 = 0.08;
        let translation = to - from;
        self.cast_moving_ball(from, translation, SIGHT_RADIUS).is_none()
    }

    #[must_use]
    pub fn ground_surface_below(&self, origin: Vec3, max_distance: f32) -> Option<WorldSurfaceHit> {
        let hit = self.surface_along_ray(origin, Vec3::NEG_Y, max_distance, ground_collision_groups())?;
        (hit.normal.y > 0.1).then_some(hit)
    }

    #[must_use]
    pub fn wall_surface_along_ray(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<WorldSurfaceHit> {
        let hit = self.surface_along_ray(origin, direction, max_distance, WALL_COLLISION_GROUP)?;
        (hit.normal.y.abs() < 0.1).then_some(hit)
    }

    // First static-world surface (wall/floor/ramp) along the ray — the same
    // filter as `line_of_sight_clear`, so a beam clipped at this point stops
    // exactly where sight does.
    #[must_use]
    pub fn world_surface_along_ray(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<WorldSurfaceHit> {
        self.surface_along_ray(origin, direction, max_distance, world_collision_groups())
    }

    fn surface_along_ray(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        groups: Group,
    ) -> Option<WorldSurfaceHit> {
        if !origin.is_finite() || !direction.is_finite() || !max_distance.is_finite() || max_distance <= 0.0 {
            return None;
        }
        let direction = direction.try_normalize()?;
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            query_filter(groups),
        );
        let ray = Ray::new(
            Vector::new(origin.x, origin.y, origin.z),
            Vector::new(direction.x, direction.y, direction.z),
        );
        let (_, hit) = query_pipeline.cast_ray_and_get_normal(&ray, max_distance, false)?;
        let normal = Vec3::new(hit.normal.x, hit.normal.y, hit.normal.z).try_normalize()?;

        Some(WorldSurfaceHit {
            point: origin + direction * hit.time_of_impact,
            normal,
        })
    }

    #[must_use]
    pub(crate) fn ground_hit(
        &self,
        character_shape: &dyn Shape,
        character_pos: &Pose,
        max_distance: f32,
        target_distance: f32,
        passable_kinds: &[BarrierKindId],
    ) -> Option<ShapeCastHit> {
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            query_filter(character_collision_groups(passable_kinds, self.all_barrier_groups)),
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
    pub(crate) fn projectile_spawn_overlaps_blocker(
        &self,
        position: Vec3,
        radius: f32,
        open_kinds: &[BarrierKindId],
    ) -> bool {
        // Walls + floors are always blockers. Barriers block the muzzle
        // unless the kind is currently open (pressure-plate held) — those
        // barriers are gone visually and shots pass through them, so the
        // muzzle clipping them is fine.
        let mut barrier_groups = self.all_barrier_groups;
        for kind in open_kinds {
            barrier_groups.remove(barrier_collision_group(*kind));
        }
        self.ball_overlaps_groups(
            position,
            radius,
            WALL_COLLISION_GROUP | FLOOR_COLLISION_GROUP | barrier_groups,
        )
    }

    #[must_use]
    pub fn cuboid_overlaps_wall(&self, position: Vec3, half_extents: Vec3) -> bool {
        self.cuboid_overlaps_groups(position, half_extents, WALL_COLLISION_GROUP)
    }

    // Whether sliding a character's body box horizontally from `start` to
    // `target` drags it through a wall. Floors and ramps are ignored so a
    // leg onto a slope counts as clear; a body already touching a wall but
    // moving away from it is clear too.
    #[must_use]
    pub fn character_sweep_hits_wall(
        &self,
        start: &Position,
        target: &Position,
        physics: CharacterPhysicsConfig,
    ) -> bool {
        let translation = Vector::new(target.x - start.x, 0.0, target.z - start.z);
        if translation.length_squared() == 0.0 {
            return false;
        }
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            query_filter(WALL_COLLISION_GROUP),
        );
        let center = character_center(*start, physics);
        let pose = Pose::translation(center.x, center.y, center.z);
        let options = ShapeCastOptions {
            max_time_of_impact: 1.0,
            stop_at_penetration: false,
            ..ShapeCastOptions::default()
        };
        query_pipeline
            .cast_shape(&pose, translation, &character_shape(physics), options)
            .is_some()
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
