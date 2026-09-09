use rapier3d::{
    parry::query::ShapeCastOptions,
    prelude::{Collider, ColliderHandle, Pose, Shape, Vector},
};

use super::{
    CollisionWorld,
    colliders::{ColliderKind, character_collision_groups, query_filter},
    shape_cast::{ShapeCastHit, upward_surface_hit},
};
use crate::protocol::{BarrierKindId, CarrierId};

impl CollisionWorld {
    #[must_use]
    pub(crate) fn ground_hit(
        &self,
        character_movement_shape: &dyn Shape,
        character_pos: &Pose,
        max_distance: f32,
        target_distance: f32,
        passable_kinds: &[BarrierKindId],
        excluded_colliders: &[ColliderHandle],
    ) -> Option<ShapeCastHit> {
        let allow = |handle: ColliderHandle, _: &Collider| !excluded_colliders.contains(&handle);
        let predicate =
            (!excluded_colliders.is_empty()).then_some(&allow as &dyn Fn(ColliderHandle, &Collider) -> bool);
        self.ground_hit_filtered(
            character_movement_shape,
            character_pos,
            max_distance,
            target_distance,
            passable_kinds,
            predicate,
        )
    }

    #[must_use]
    pub(crate) fn ground_hit_on_carrier(
        &self,
        character_movement_shape: &dyn Shape,
        character_pos: &Pose,
        max_distance: f32,
        passable_kinds: &[BarrierKindId],
        carrier: CarrierId,
    ) -> Option<ShapeCastHit> {
        let on_carrier = |_: ColliderHandle, collider: &Collider| {
            ColliderKind::carrier_from_user_data(collider.user_data) == carrier
        };
        self.ground_hit_filtered(
            character_movement_shape,
            character_pos,
            max_distance,
            0.0,
            passable_kinds,
            Some(&on_carrier),
        )
    }

    fn ground_hit_filtered(
        &self,
        character_movement_shape: &dyn Shape,
        character_pos: &Pose,
        max_distance: f32,
        target_distance: f32,
        passable_kinds: &[BarrierKindId],
        predicate: Option<&dyn Fn(ColliderHandle, &Collider) -> bool>,
    ) -> Option<ShapeCastHit> {
        let mut filter = query_filter(character_collision_groups(passable_kinds, self.all_barrier_groups));
        filter.predicate = predicate;
        let options = ShapeCastOptions {
            max_time_of_impact: max_distance,
            target_distance,
            stop_at_penetration: false,
            compute_impact_geometry_on_penetration: true,
        };

        self.query_pipeline(filter)
            .cast_shape(character_pos, Vector::NEG_Y, character_movement_shape, options)
            .and_then(|(handle, hit)| upward_surface_hit(hit, self.carrier_of(handle)))
    }
}
