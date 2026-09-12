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
    constants::{CHARACTER_CONTACT_OFFSET, CHARACTER_MAX_SLOPE, CHARACTER_STEP_HEIGHT},
    math::PHYSICS_EPSILON,
    physics::characters::{character_controller, character_movement_pose, character_movement_shape},
    protocol::{BarrierId, Position},
};

impl CollisionWorld {
    #[must_use]
    // Use the motor's step and slope rules: a straight capsule sweep would reject walkable ramps.
    pub fn character_ground_route_clear(
        &self,
        start: Position,
        target: Position,
        physics: CharacterPhysicsConfig,
        open: &[BarrierId],
    ) -> bool {
        let start = self.ground_route_position(start, physics, open);
        let target = self.ground_route_position(target, physics, open);
        let translation = Vector::new(target.x - start.x, target.y - start.y, target.z - start.z);
        let horizontal = translation.with_y(0.0);
        let controller = character_controller();
        let shape = character_movement_shape(physics);
        let mut position = start;
        // Slope transitions consume part of a sweep while redirecting it onto the walking surface.
        for _ in 0..4 {
            let remaining = Vector::new(target.x - position.x, target.y - position.y, target.z - position.z);
            let movement = self.move_character(
                1.0,
                &controller,
                &shape,
                &character_movement_pose(&position, physics),
                remaining,
                open,
                &[],
                |_| {},
            );
            position.x += movement.translation.x;
            position.y += movement.translation.y;
            position.z += movement.translation.z;
            let offset = Vector::new(position.x - start.x, 0.0, position.z - start.z);
            let lateral =
                offset - horizontal * (offset.dot(horizontal) / horizontal.length_squared().max(PHYSICS_EPSILON));
            if lateral.length_squared() > 0.05 * 0.05 {
                return false;
            }
            let error = movement.translation - remaining;
            if error.with_y(0.0).length_squared() <= 0.05 * 0.05 && error.y.abs() <= CHARACTER_STEP_HEIGHT {
                return !self.character_penetrates_solid(&position, physics, open);
            }
            if movement.translation.with_y(0.0).length_squared() <= PHYSICS_EPSILON * PHYSICS_EPSILON {
                return false;
            }
        }
        false
    }

    fn ground_route_position(
        &self,
        mut position: Position,
        physics: CharacterPhysicsConfig,
        open: &[BarrierId],
    ) -> Position {
        let lift = physics.movement_collider.radius() + CHARACTER_STEP_HEIGHT;
        let mut pose = character_movement_pose(&position, physics);
        pose.translation.y += lift;
        let Some(hit) = self.ground_hit(
            &character_movement_shape(physics),
            &pose,
            lift + CHARACTER_CONTACT_OFFSET,
            0.0,
            open,
            &[],
        ) else {
            return position;
        };
        if hit.normal.y < CHARACTER_MAX_SLOPE.cos() {
            return position;
        }
        let adjustment = lift - hit.t + CHARACTER_CONTACT_OFFSET / hit.normal.y;
        // Graph heights describe surfaces; an upright capsule's rounded base stands higher on a slope.
        let slope_clearance = physics.movement_collider.radius() * (hit.normal.y.recip() - 1.0);
        if adjustment.abs() <= CHARACTER_STEP_HEIGHT + slope_clearance + CHARACTER_CONTACT_OFFSET * 2.0 {
            position.y += adjustment;
        }
        position
    }

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

    pub fn character_penetrates_solid(
        &self,
        pos: &Position,
        physics: CharacterPhysicsConfig,
        open: &[BarrierId],
    ) -> bool {
        // Contact skin is harmless; crushing requires geometry inside the body.
        let margin = CHARACTER_CONTACT_OFFSET * 0.1;
        let mut inset = physics;
        inset.movement_collider.diameter = (physics.movement_collider.diameter - margin * 2.0).max(PHYSICS_EPSILON);
        inset.movement_collider.height =
            (physics.movement_collider.height - margin * 2.0).max(inset.movement_collider.diameter);
        self.character_overlaps_solid(
            &Position {
                y: pos.y + margin,
                ..*pos
            },
            inset,
            open,
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

#[cfg(test)]
#[path = "tests/character_queries.rs"]
mod tests;
