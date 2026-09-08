use rapier3d::{
    control::{CharacterCollision, KinematicCharacterController},
    parry::{query::contact, shape::Capsule},
    prelude::{Collider, ColliderHandle, Pose, Shape, Vector},
};

use super::{
    CollisionWorld,
    colliders::{ColliderKind, character_collision_groups, query_filter},
};
use crate::{
    config::CharacterPhysicsConfig,
    constants::CHARACTER_CONTACT_OFFSET,
    map::Carriers,
    math::PHYSICS_EPSILON,
    physics::characters::{character_movement_center, character_movement_shape},
    protocol::{BarrierKindId, Position},
};

impl CollisionWorld {
    pub(crate) fn push_character_from_carriers(
        &self,
        dt: f32,
        controller: &KinematicCharacterController,
        shape: &dyn Shape,
        start: &Pose,
        carriers: &Carriers,
        passable_kinds: &[BarrierKindId],
        excluded_colliders: &[ColliderHandle],
        mut events: impl FnMut(CharacterCollision),
    ) -> Vector {
        let allow = |handle: ColliderHandle, collider: &Collider| {
            !excluded_colliders.contains(&handle)
                && ColliderKind::from_user_data(collider.user_data) != Some(ColliderKind::Ramp)
                && carriers
                    .displacement(self.carrier_of(handle))
                    .with_y(0.0)
                    .length_squared()
                    > PHYSICS_EPSILON * PHYSICS_EPSILON
        };
        let mut filter = query_filter(character_collision_groups(passable_kinds, self.all_barrier_groups));
        filter.predicate = Some(&allow);
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            filter,
        );
        let mut pose = *start;
        let mut overlaps: Vec<_> = query_pipeline
            .intersect_shape(pose, shape)
            .map(|(handle, _)| handle)
            .collect();
        overlaps.sort_unstable_by_key(|handle| handle.into_raw_parts());
        for handle in overlaps {
            let collider = &self.colliders[handle];
            let Ok(Some(hit)) = contact(collider.position(), collider.shape(), &pose, shape, 0.0) else {
                continue;
            };
            let normal = hit.normal1.with_y(0.0);
            let travel = carriers.displacement(self.carrier_of(handle));
            if hit.dist >= 0.0
                || hit.normal1.y.abs() > 0.5
                || normal.x * travel.x + normal.z * travel.z <= PHYSICS_EPSILON
            {
                continue;
            }
            // Only the incoming face pushes; tangential travel must not drag a bystander along the wall.
            let push = normal * ((CHARACTER_CONTACT_OFFSET - hit.dist) / normal.length_squared());
            let resolved = self.move_character(
                dt,
                controller,
                shape,
                &pose,
                push,
                passable_kinds,
                excluded_colliders,
                &mut events,
            );
            pose.translation += resolved.translation;
        }
        pose.translation - start.translation
    }

    pub(crate) fn character_crushed(
        &self,
        pos: &Position,
        physics: CharacterPhysicsConfig,
        passable_kinds: &[BarrierKindId],
        excluded_colliders: &[ColliderHandle],
        lifted: bool,
    ) -> bool {
        let shape = character_movement_shape(physics);
        let center = character_movement_center(*pos, physics);
        let inset = Capsule {
            radius: (shape.radius - CHARACTER_CONTACT_OFFSET * 2.0).max(PHYSICS_EPSILON),
            ..shape
        };
        let allow = |handle: ColliderHandle, collider: &Collider| {
            !excluded_colliders.contains(&handle)
                && ColliderKind::from_user_data(collider.user_data) != Some(ColliderKind::Ramp)
                && (lifted || !self.carrier_of(handle).is_world())
        };
        let mut filter = query_filter(character_collision_groups(passable_kinds, self.all_barrier_groups));
        filter.predicate = Some(&allow);
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            filter,
        );
        query_pipeline
            .intersect_shape(Pose::translation(center.x, center.y, center.z), &inset)
            .next()
            .is_some()
    }
}
