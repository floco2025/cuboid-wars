use rapier3d::{
    control::{CharacterCollision, EffectiveCharacterMovement, KinematicCharacterController},
    parry::query::{
        ClosestPoints, Contact, NonlinearRigidMotion, QueryDispatcher, ShapeCastHit, ShapeCastOptions, Unsupported,
    },
    prelude::{Collider, ColliderHandle, Pose, Shape, Vector},
};

use super::{
    CollisionWorld,
    colliders::{WALL_COLLISION_GROUP, barrier_blocks, character_collision_groups, query_filter},
};
use crate::{
    config::CharacterPhysicsConfig,
    physics::characters::{character_movement_pose, character_movement_shape},
    protocol::{BarrierId, Position},
};

impl CollisionWorld {
    #[must_use]
    pub(crate) fn move_character(
        &self,
        dt: f32,
        controller: &KinematicCharacterController,
        character_movement_shape: &dyn Shape,
        character_pos: &Pose,
        desired_translation: Vector,
        passable_kinds: &[BarrierId],
        excluded_colliders: &[ColliderHandle],
        events: impl FnMut(CharacterCollision),
    ) -> EffectiveCharacterMovement {
        // Portal transit: the aperture's backing colliders stop existing for
        // a body overlapping the aperture, which is what lets it pass
        // through the surface.
        let allow = |handle: ColliderHandle, collider: &Collider| {
            !excluded_colliders.contains(&handle) && barrier_blocks(collider, passable_kinds)
        };
        let mut filter = query_filter(character_collision_groups());
        filter.predicate = Some(&allow);
        let dispatcher = CharacterQueryDispatcher(self.narrow_phase.query_dispatcher());
        let query_pipeline = self
            .broad_phase
            .as_query_pipeline(&dispatcher, &self.bodies, &self.colliders, filter);
        controller.move_shape(
            dt,
            &query_pipeline,
            character_movement_shape,
            character_pos,
            desired_translation,
            events,
        )
    }

    #[must_use]
    pub fn character_overlaps_wall(&self, pos: &Position, physics: CharacterPhysicsConfig) -> bool {
        self.shape_overlaps(
            character_movement_pose(pos, physics),
            &character_movement_shape(physics),
            query_filter(WALL_COLLISION_GROUP),
        )
    }

    #[must_use]
    pub fn character_overlaps_solid(
        &self,
        pos: &Position,
        physics: CharacterPhysicsConfig,
        passable_kinds: &[BarrierId],
    ) -> bool {
        let allow = |_: ColliderHandle, collider: &Collider| barrier_blocks(collider, passable_kinds);
        let mut filter = query_filter(character_collision_groups());
        filter.predicate = Some(&allow);
        self.shape_overlaps(
            character_movement_pose(pos, physics),
            &character_movement_shape(physics),
            filter,
        )
    }

    // Whether sliding a character's movement capsule horizontally from `start` to
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
        let options = ShapeCastOptions {
            max_time_of_impact: 1.0,
            stop_at_penetration: false,
            ..ShapeCastOptions::default()
        };
        self.query_pipeline(query_filter(WALL_COLLISION_GROUP))
            .cast_shape(
                &character_movement_pose(start, physics),
                translation,
                &character_movement_shape(physics),
                options,
            )
            .is_some()
    }
}

pub(super) struct CharacterQueryDispatcher<'a>(pub &'a dyn QueryDispatcher);

impl QueryDispatcher for CharacterQueryDispatcher<'_> {
    fn intersection_test(&self, pose: &Pose, a: &dyn Shape, b: &dyn Shape) -> Result<bool, Unsupported> {
        self.0.intersection_test(pose, a, b)
    }

    fn distance(&self, pose: &Pose, a: &dyn Shape, b: &dyn Shape) -> Result<f32, Unsupported> {
        self.0.distance(pose, a, b)
    }

    fn contact(
        &self,
        pose: &Pose,
        a: &dyn Shape,
        b: &dyn Shape,
        prediction: f32,
    ) -> Result<Option<Contact>, Unsupported> {
        self.0.contact(pose, a, b, prediction)
    }

    fn closest_points(
        &self,
        pose: &Pose,
        a: &dyn Shape,
        b: &dyn Shape,
        max_dist: f32,
    ) -> Result<ClosestPoints, Unsupported> {
        self.0.closest_points(pose, a, b, max_dist)
    }

    fn cast_shapes(
        &self,
        pose: &Pose,
        velocity: Vector,
        a: &dyn Shape,
        b: &dyn Shape,
        options: ShapeCastOptions,
    ) -> Result<Option<ShapeCastHit>, Unsupported> {
        let mut hit = self.0.cast_shapes(pose, velocity, a, b, options)?;
        if let Some(hit) = hit.as_mut() {
            let mut impact_pose = *pose;
            impact_pose.translation += velocity * hit.time_of_impact;
            // Rapier's slope decomposition discards forward motion on imprecise capsule cast normals.
            if let Some(contact) = self.0.contact(&impact_pose, a, b, f32::MAX)? {
                hit.normal1 = contact.normal1.normalize_or_zero();
                hit.normal2 = contact.normal2.normalize_or_zero();
                hit.witness1 = contact.point1;
                hit.witness2 = contact.point2;
            }
        }
        Ok(hit)
    }

    fn cast_shapes_nonlinear(
        &self,
        motion1: &NonlinearRigidMotion,
        a: &dyn Shape,
        motion2: &NonlinearRigidMotion,
        b: &dyn Shape,
        start: f32,
        end: f32,
        stop: bool,
    ) -> Result<Option<ShapeCastHit>, Unsupported> {
        self.0.cast_shapes_nonlinear(motion1, a, motion2, b, start, end, stop)
    }
}
