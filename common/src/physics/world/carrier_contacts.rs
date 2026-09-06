use bevy_math::Vec3;
use rapier3d::{
    control::{CharacterCollision, KinematicCharacterController},
    parry::{
        query::{contact, intersection_test},
        shape::Cuboid,
    },
    prelude::{Collider, ColliderHandle, Pose, Shape, Vector},
};

use super::{
    CollisionWorld,
    colliders::{ColliderKind, character_collision_groups, query_filter},
};
use crate::{
    config::CharacterPhysicsConfig,
    constants::{CHARACTER_CONTACT_OFFSET, PHYSICS_EPSILON},
    map::Carriers,
    physics::characters::{character_center, character_shape},
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

    // Leg-only contact permits boarding raised slabs. A carried collider must
    // penetrate the movement box before or after control movement to count; checking
    // the full height afterwards prevents escaping through a descending slab.
    // Vertical carry also checks static geometry, such as a lift's ceiling.
    pub(crate) fn character_crushed(
        &self,
        start: &Position,
        pos: &Position,
        physics: CharacterPhysicsConfig,
        passable_kinds: &[BarrierKindId],
        excluded_colliders: &[ColliderHandle],
        lifted: bool,
    ) -> bool {
        let collision_box = character_shape(physics);
        let center = character_center(*pos, physics);
        let start_center = character_center(*start, physics);
        let movement_box = inset_contact_shape(&collision_box);
        let movement_poses = [
            Pose::translation(start_center.x, start_center.y, start_center.z),
            Pose::translation(center.x, center.y, center.z),
        ];
        let head = center.y + collision_box.half_extents.y;
        let feet = pos.y.min(center.y - collision_box.half_extents.y);
        let body = Cuboid::new(Vector::new(
            collision_box.half_extents.x,
            (head - feet) / 2.0,
            collision_box.half_extents.z,
        ));
        let body_center = Vec3::new(center.x, (head + feet) / 2.0, center.z);
        let carried = |_: ColliderHandle, collider: &Collider| {
            !ColliderKind::carrier_from_user_data(collider.user_data).is_world()
                && movement_poses.iter().any(|pose| {
                    intersection_test(pose, &movement_box, collider.position(), collider.shape())
                        .is_ok_and(|overlaps| overlaps)
                })
        };
        let any = |_: ColliderHandle, _: &Collider| true;
        self.crushing_overlap(body_center, &body, passable_kinds, excluded_colliders, &carried)
            || (lifted && self.crushing_overlap(center, &collision_box, passable_kinds, excluded_colliders, &any))
    }

    fn crushing_overlap(
        &self,
        center: Vec3,
        shape: &Cuboid,
        passable_kinds: &[BarrierKindId],
        excluded_colliders: &[ColliderHandle],
        counts: &dyn Fn(ColliderHandle, &Collider) -> bool,
    ) -> bool {
        let allow = |handle: ColliderHandle, collider: &Collider| {
            !excluded_colliders.contains(&handle)
                && ColliderKind::from_user_data(collider.user_data) != Some(ColliderKind::Ramp)
                && counts(handle, collider)
        };
        let mut filter = query_filter(character_collision_groups(passable_kinds, self.all_barrier_groups));
        filter.predicate = Some(&allow);
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            filter,
        );
        let pose = Pose::translation(center.x, center.y, center.z);
        query_pipeline
            .intersect_shape(pose, &inset_contact_shape(shape))
            .next()
            .is_some()
    }
}

// Contact margins tolerate resting touches; only penetration counts as crushing.
fn inset_contact_shape(shape: &Cuboid) -> Cuboid {
    let inset = CHARACTER_CONTACT_OFFSET * 2.0;
    Cuboid::new(Vector::new(
        (shape.half_extents.x - inset).max(PHYSICS_EPSILON),
        (shape.half_extents.y - inset).max(PHYSICS_EPSILON),
        (shape.half_extents.z - inset).max(PHYSICS_EPSILON),
    ))
}
