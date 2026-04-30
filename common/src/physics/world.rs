use bevy_ecs::prelude::Resource;
use bevy_math::Vec3;
use rapier3d::{
    control::{CharacterCollision, EffectiveCharacterMovement, KinematicCharacterController},
    parry::{
        query::{ShapeCastHit as RapierShapeCastHit, ShapeCastOptions},
        shape::{Ball, Cuboid},
    },
    prelude::{
        BroadPhaseBvh, ColliderSet, Group, IntegrationParameters, NarrowPhase, Pose, RigidBodySet, Shape, Vector,
    },
};

use crate::protocol::MapLayout;

use self::colliders::{
    FLOOR_COLLISION_GROUP, WALL_COLLISION_GROUP, character_collision_groups, insert_floor_collider,
    insert_ramp_collider, insert_wall_collider, query_filter, world_collision_groups,
};

mod colliders;

#[cfg(test)]
use self::colliders::ColliderKind;

#[derive(Resource)]
pub struct CollisionWorld {
    bodies: RigidBodySet,
    colliders: ColliderSet,
    broad_phase: BroadPhaseBvh,
    narrow_phase: NarrowPhase,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ShapeCastHit {
    pub normal: Vec3,
    pub t: f32,
}

impl CollisionWorld {
    #[must_use]
    pub fn from_map_layout(map_layout: &MapLayout) -> Self {
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

        Self {
            bodies,
            colliders,
            broad_phase,
            narrow_phase,
        }
    }

    #[cfg(test)]
    #[must_use]
    fn solid_count(&self) -> usize {
        self.colliders.len()
    }

    #[cfg(test)]
    #[must_use]
    fn solid_kinds(&self) -> Vec<ColliderKind> {
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
        events: impl FnMut(CharacterCollision),
    ) -> EffectiveCharacterMovement {
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            query_filter(character_collision_groups(has_phasing)),
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

    #[must_use]
    pub(crate) fn cast_moving_ball(&self, position: Vec3, translation: Vec3, radius: f32) -> Option<ShapeCastHit> {
        if translation.length_squared() == 0.0 {
            return None;
        }

        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            query_filter(world_collision_groups()),
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

    #[must_use]
    pub(crate) fn ground_hit(
        &self,
        character_shape: &dyn Shape,
        character_pos: &Pose,
        max_distance: f32,
        target_distance: f32,
        has_phasing: bool,
    ) -> Option<ShapeCastHit> {
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            query_filter(character_collision_groups(has_phasing)),
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

fn upward_surface_hit(hit: RapierShapeCastHit) -> Option<ShapeCastHit> {
    [hit.normal1, hit.normal2, -hit.normal1, -hit.normal2]
        .into_iter()
        .map(|normal| Vec3::new(normal.x, normal.y, normal.z))
        .max_by(|a, b| a.y.total_cmp(&b.y))
        .filter(|normal| normal.y > 0.1)
        .and_then(|normal| normal.try_normalize())
        .map(|normal| ShapeCastHit {
            normal,
            t: hit.time_of_impact,
        })
}

#[cfg(test)]
mod tests;
