use bevy_math::Vec3;
use rapier3d::{
    parry::query::{ShapeCastOptions, contact},
    prelude::{Collider, ColliderHandle, Pose},
};

use super::{
    CollisionWorld,
    colliders::{barrier_blocks, character_collision_groups, query_filter},
};
use crate::{
    config::CharacterPhysicsConfig,
    constants::CHARACTER_CONTACT_OFFSET,
    map::Carriers,
    math::{PHYSICS_EPSILON, from_rapier, to_rapier},
    physics::{
        CharacterMovementResult, CharacterSupport, GroundingDiagnostics,
        characters::{character_movement_pose, character_movement_shape},
    },
    protocol::{BarrierId, CarrierId, Position},
};

impl CollisionWorld {
    pub fn flight_search_bounds(&self, start: Vec3, target: Vec3, margin: f32) -> (Vec3, Vec3) {
        let mut min = start.min(target);
        let mut max = start.max(target);
        for (_, collider) in self.colliders.iter() {
            let bounds = collider.compute_aabb();
            min = min.min(from_rapier(bounds.mins));
            max = max.max(from_rapier(bounds.maxs));
        }
        (min - Vec3::splat(margin), max + Vec3::splat(margin))
    }

    pub fn character_flight_path_clear(
        &self,
        start: Position,
        end: Position,
        physics: CharacterPhysicsConfig,
        open: &[BarrierId],
    ) -> bool {
        !self.character_overlaps_solid(&start, physics, open)
            && !self.character_overlaps_solid(&end, physics, open)
            && self
                .flight_cast(
                    &character_movement_pose(&start, physics),
                    Vec3::from(end) - Vec3::from(start),
                    physics,
                    open,
                    &[],
                )
                .is_none()
    }

    pub fn move_flying_character(
        &self,
        start: Position,
        translation: Vec3,
        delta: f32,
        physics: CharacterPhysicsConfig,
        open: &[BarrierId],
        carriers: &Carriers,
    ) -> CharacterMovementResult {
        let mut pose = character_movement_pose(&start, physics);
        let initial = pose;
        let shape = character_movement_shape(physics);
        let allow = |handle: ColliderHandle, collider: &Collider| {
            barrier_blocks(collider, open)
                && carriers.displacement(self.carrier_of(handle)).length_squared() > PHYSICS_EPSILON * PHYSICS_EPSILON
        };
        let mut filter = query_filter(character_collision_groups());
        filter.predicate = Some(&allow);
        let mut overlaps: Vec<_> = self
            .query_pipeline(filter)
            .intersect_shape(pose, &shape)
            .map(|(handle, _)| handle)
            .collect();
        overlaps.sort_unstable_by_key(|handle| handle.into_raw_parts());
        let mut pushed = false;
        for handle in overlaps {
            let collider = &self.colliders[handle];
            let Ok(Some(hit)) = contact(collider.position(), collider.shape(), &pose, &shape, 0.0) else {
                continue;
            };
            let normal = from_rapier(hit.normal1);
            if hit.dist >= 0.0 || normal.dot(carriers.displacement(self.carrier_of(handle))) <= 0.0 {
                continue;
            }
            let push = normal * (CHARACTER_CONTACT_OFFSET - hit.dist);
            let resolved = self.slide_flying_body(pose, push, physics, open, &[handle]);
            pose.translation += to_rapier(resolved);
            pushed = true;
        }
        let resolved = self.slide_flying_body(pose, translation, physics, open, &[]);
        pose.translation += to_rapier(resolved);
        let traveled = from_rapier(pose.translation - initial.translation);
        let position = Position::from(Vec3::from(start) + traveled);
        let inset_margin = CHARACTER_CONTACT_OFFSET * 0.1;
        let mut inset = physics;
        inset.movement_collider.diameter =
            (physics.movement_collider.diameter - inset_margin * 2.0).max(PHYSICS_EPSILON);
        inset.movement_collider.height =
            (physics.movement_collider.height - inset_margin * 2.0).max(inset.movement_collider.diameter);
        let inset_position = Position::from(Vec3::from(position) + Vec3::Y * inset_margin);
        CharacterMovementResult {
            grounding: GroundingDiagnostics::default(),
            position,
            vertical_velocity: traveled.y / delta,
            impact_speed: 0.0,
            support: CharacterSupport::Airborne,
            blocked: resolved.distance_squared(translation) > PHYSICS_EPSILON * PHYSICS_EPSILON,
            carrier: CarrierId::WORLD,
            floor_velocity: Vec3::ZERO,
            lifted: false,
            crushed: pushed && self.character_overlaps_solid(&inset_position, inset, open),
        }
    }

    fn slide_flying_body(
        &self,
        mut pose: Pose,
        mut remaining: Vec3,
        physics: CharacterPhysicsConfig,
        open: &[BarrierId],
        excluded: &[ColliderHandle],
    ) -> Vec3 {
        let start = pose.translation;
        for _ in 0..4 {
            let distance = remaining.length();
            if distance <= PHYSICS_EPSILON {
                break;
            }
            let Some((t, normal)) = self.flight_cast(&pose, remaining, physics, open, excluded) else {
                pose.translation += to_rapier(remaining);
                break;
            };
            let fraction = (t - CHARACTER_CONTACT_OFFSET / distance).clamp(0.0, 1.0);
            pose.translation += to_rapier(remaining * fraction);
            remaining *= 1.0 - fraction;
            remaining -= normal * remaining.dot(normal).min(0.0);
        }
        from_rapier(pose.translation - start)
    }

    fn flight_cast(
        &self,
        pose: &Pose,
        translation: Vec3,
        physics: CharacterPhysicsConfig,
        open: &[BarrierId],
        excluded: &[ColliderHandle],
    ) -> Option<(f32, Vec3)> {
        if translation.length_squared() <= PHYSICS_EPSILON * PHYSICS_EPSILON {
            return None;
        }
        let allow =
            |handle: ColliderHandle, collider: &Collider| !excluded.contains(&handle) && barrier_blocks(collider, open);
        let mut filter = query_filter(character_collision_groups());
        filter.predicate = Some(&allow);
        self.query_pipeline(filter)
            .cast_shape(
                pose,
                to_rapier(translation),
                &character_movement_shape(physics),
                ShapeCastOptions {
                    max_time_of_impact: 1.0,
                    stop_at_penetration: false,
                    ..ShapeCastOptions::default()
                },
            )
            .map(|(_, hit)| {
                let mut normal = from_rapier(hit.normal2).normalize_or_zero();
                if normal.dot(translation) > 0.0 {
                    normal = -normal;
                }
                (hit.time_of_impact, normal)
            })
    }
}

#[cfg(test)]
#[path = "tests/flight.rs"]
mod tests;
