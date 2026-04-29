use bevy_ecs::prelude::Resource;
use bevy_math::Vec3;
use rapier3d::{
    control::{CharacterCollision, EffectiveCharacterMovement, KinematicCharacterController},
    parry::{
        query::{ShapeCastHit as RapierShapeCastHit, ShapeCastOptions},
        shape::Ball,
    },
    prelude::{
        BroadPhaseBvh, ColliderBuilder, ColliderHandle, ColliderSet, Group, IntegrationParameters, InteractionGroups,
        InteractionTestMode, NarrowPhase, Pose, QueryFilter, RigidBodySet, Shape, Vector,
    },
};

use crate::{
    constants::{LEVEL_HEIGHT, WALL_HEIGHT},
    map::{RampAxis, ramp_axis},
    protocol::{Floor, MapLayout, Ramp, Wall},
};

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

const WALL_COLLISION_GROUP: Group = Group::GROUP_1;
const FLOOR_COLLISION_GROUP: Group = Group::GROUP_2;
const RAMP_COLLISION_GROUP: Group = Group::GROUP_3;

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
        include_floors: bool,
        events: impl FnMut(CharacterCollision),
    ) -> EffectiveCharacterMovement {
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            query_filter(character_collision_groups(has_phasing, include_floors)),
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
            query_filter(character_collision_groups(has_phasing, true)),
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
        let shape = rapier3d::parry::shape::Cuboid::new(Vector::new(half_extents.x, half_extents.y, half_extents.z));
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

fn world_collision_groups() -> Group {
    WALL_COLLISION_GROUP | FLOOR_COLLISION_GROUP | RAMP_COLLISION_GROUP
}

fn character_collision_groups(has_phasing: bool, include_floors: bool) -> Group {
    let mut groups = world_collision_groups();

    if has_phasing {
        groups.remove(WALL_COLLISION_GROUP);
    }

    if !include_floors {
        groups.remove(FLOOR_COLLISION_GROUP);
    }

    groups
}

fn query_filter(groups: Group) -> QueryFilter<'static> {
    InteractionGroups::new(Group::ALL, groups, InteractionTestMode::And).into()
}

fn collider_interaction_groups(kind: ColliderKind) -> InteractionGroups {
    InteractionGroups::new(kind.group(), Group::ALL, InteractionTestMode::And)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColliderKind {
    Wall,
    Floor,
    Ramp,
}

impl ColliderKind {
    fn group(self) -> Group {
        match self {
            Self::Wall => WALL_COLLISION_GROUP,
            Self::Floor => FLOOR_COLLISION_GROUP,
            Self::Ramp => RAMP_COLLISION_GROUP,
        }
    }

    fn user_data(self) -> u128 {
        match self {
            Self::Wall => 1,
            Self::Floor => 2,
            Self::Ramp => 3,
        }
    }

    #[cfg(test)]
    fn from_user_data(user_data: u128) -> Option<Self> {
        match user_data {
            1 => Some(Self::Wall),
            2 => Some(Self::Floor),
            3 => Some(Self::Ramp),
            _ => None,
        }
    }
}

fn insert_wall_collider(colliders: &mut ColliderSet, wall: &Wall) -> ColliderHandle {
    let dx = (wall.x2 - wall.x1).abs();
    let dz = (wall.z2 - wall.z1).abs();
    let wall_half_thickness = wall.width / 2.0;
    let is_horizontal = dx > dz;
    let half_extents = Vec3::new(
        if is_horizontal { dx / 2.0 } else { wall_half_thickness },
        WALL_HEIGHT / 2.0,
        if is_horizontal { wall_half_thickness } else { dz / 2.0 },
    );
    let center = Vec3::new(
        f32::midpoint(wall.x1, wall.x2),
        f32::from(wall.level).mul_add(LEVEL_HEIGHT, WALL_HEIGHT / 2.0),
        f32::midpoint(wall.z1, wall.z2),
    );

    insert_cuboid_collider(colliders, center, half_extents, ColliderKind::Wall)
}

fn insert_floor_collider(colliders: &mut ColliderSet, floor: &Floor) -> ColliderHandle {
    let (min_x, max_x, min_z, max_z) = floor.bounds_xz();
    let center = Vec3::new(
        f32::midpoint(min_x, max_x),
        floor.y - floor.thickness / 2.0,
        f32::midpoint(min_z, max_z),
    );
    let half_extents = Vec3::new((max_x - min_x) / 2.0, floor.thickness / 2.0, (max_z - min_z) / 2.0);

    insert_cuboid_collider(colliders, center, half_extents, ColliderKind::Floor)
}

fn insert_cuboid_collider(
    colliders: &mut ColliderSet,
    center: Vec3,
    half_extents: Vec3,
    kind: ColliderKind,
) -> ColliderHandle {
    colliders.insert(
        ColliderBuilder::cuboid(half_extents.x, half_extents.y, half_extents.z)
            .position(Pose::translation(center.x, center.y, center.z))
            .collision_groups(collider_interaction_groups(kind))
            .user_data(kind.user_data())
            .build(),
    )
}

fn insert_ramp_collider(colliders: &mut ColliderSet, ramp: &Ramp) -> Option<ColliderHandle> {
    let (min_x, max_x, min_z, max_z) = ramp.bounds_xz();
    let (min_y, max_y) = ramp.bounds_y();
    let high_is_second = ramp.y2 >= ramp.y1;
    let points = match ramp_axis(ramp) {
        RampAxis::X => {
            let high_x = if high_is_second { ramp.x2 } else { ramp.x1 };
            vec![
                Vector::new(min_x, min_y, min_z),
                Vector::new(min_x, min_y, max_z),
                Vector::new(max_x, min_y, min_z),
                Vector::new(max_x, min_y, max_z),
                Vector::new(high_x, max_y, min_z),
                Vector::new(high_x, max_y, max_z),
            ]
        }
        RampAxis::Z => {
            let high_z = if high_is_second { ramp.z2 } else { ramp.z1 };
            vec![
                Vector::new(min_x, min_y, min_z),
                Vector::new(max_x, min_y, min_z),
                Vector::new(min_x, min_y, max_z),
                Vector::new(max_x, min_y, max_z),
                Vector::new(min_x, max_y, high_z),
                Vector::new(max_x, max_y, high_z),
            ]
        }
    };

    let collider = ColliderBuilder::convex_hull(&points)?
        .collision_groups(collider_interaction_groups(ColliderKind::Ramp))
        .user_data(ColliderKind::Ramp.user_data())
        .build();
    Some(colliders.insert(collider))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS};

    fn test_map_layout() -> MapLayout {
        MapLayout {
            walls: vec![Wall {
                x1: 0.0,
                z1: 0.0,
                x2: 4.0,
                z2: 0.0,
                width: WALL_THICKNESS,
                level: 1,
            }],
            floors: vec![Floor {
                x1: 0.0,
                z1: 0.0,
                x2: 4.0,
                z2: 4.0,
                y: LEVEL_HEIGHT,
                thickness: FLOOR_THICKNESS,
                level: 1,
            }],
            ramps: vec![Ramp {
                x1: 0.0,
                y1: 0.0,
                z1: 0.0,
                x2: 4.0,
                y2: LEVEL_HEIGHT,
                z2: 8.0,
            }],
            wall_lights: vec![],
        }
    }

    #[test]
    fn collision_world_contains_solids_for_walls_floors_and_ramps() {
        let world = CollisionWorld::from_map_layout(&test_map_layout());

        assert_eq!(world.solid_count(), 3);
        assert_eq!(
            world.solid_kinds(),
            vec![ColliderKind::Wall, ColliderKind::Floor, ColliderKind::Ramp]
        );
    }

    #[test]
    fn wall_solid_uses_wall_level_height() {
        let world = CollisionWorld::from_map_layout(&test_map_layout());
        let (_, wall_collider) = world
            .colliders
            .iter()
            .find(|(_, collider)| ColliderKind::from_user_data(collider.user_data) == Some(ColliderKind::Wall))
            .expect("expected wall collider");
        let wall_center_y = wall_collider.position().translation.y;

        assert_eq!(wall_center_y, LEVEL_HEIGHT + WALL_HEIGHT / 2.0);
    }

    #[test]
    fn ramp_converts_to_collider() {
        let world = CollisionWorld::from_map_layout(&test_map_layout());

        assert!(world.solid_kinds().contains(&ColliderKind::Ramp));
    }
}
