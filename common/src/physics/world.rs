use bevy_ecs::prelude::Resource;
use rapier3d::{
    control::{CharacterCollision, EffectiveCharacterMovement, KinematicCharacterController},
    prelude::{
        BroadPhaseBvh, Collider, ColliderBuilder, ColliderHandle, ColliderSet, IntegrationParameters, NarrowPhase,
        Pose, QueryFilter, RigidBodySet, Shape, Vector,
    },
};

use super::{Cuboid, floor_cuboid, wall_cuboid};
use crate::{
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

    #[must_use]
    pub fn solid_count(&self) -> usize {
        self.colliders.len()
    }

    #[must_use]
    pub fn solid_kinds(&self) -> Vec<ColliderKind> {
        self.colliders
            .iter()
            .filter_map(|(_, collider)| ColliderKind::from_user_data(collider.user_data))
            .collect()
    }

    #[must_use]
    pub fn collider_kind(&self, handle: ColliderHandle) -> Option<ColliderKind> {
        self.colliders
            .get(handle)
            .and_then(|collider| ColliderKind::from_user_data(collider.user_data))
    }

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
        if has_phasing || !include_floors {
            let include = |_: ColliderHandle, collider: &Collider| {
                let kind = ColliderKind::from_user_data(collider.user_data);
                !(has_phasing && kind == Some(ColliderKind::Wall))
                    && (include_floors || kind != Some(ColliderKind::Floor))
            };
            let query_pipeline = self.broad_phase.as_query_pipeline(
                self.narrow_phase.query_dispatcher(),
                &self.bodies,
                &self.colliders,
                QueryFilter::default().predicate(&include),
            );
            controller.move_shape(
                dt,
                &query_pipeline,
                character_shape,
                character_pos,
                desired_translation,
                events,
            )
        } else {
            let query_pipeline = self.broad_phase.as_query_pipeline(
                self.narrow_phase.query_dispatcher(),
                &self.bodies,
                &self.colliders,
                QueryFilter::default(),
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
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColliderKind {
    Wall,
    Floor,
    Ramp,
}

impl ColliderKind {
    fn user_data(self) -> u128 {
        match self {
            Self::Wall => 1,
            Self::Floor => 2,
            Self::Ramp => 3,
        }
    }

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
    insert_cuboid_collider(colliders, wall_cuboid(wall, 0.0), ColliderKind::Wall)
}

fn insert_floor_collider(colliders: &mut ColliderSet, floor: &Floor) -> ColliderHandle {
    insert_cuboid_collider(colliders, floor_cuboid(floor, 0.0), ColliderKind::Floor)
}

fn insert_cuboid_collider(colliders: &mut ColliderSet, cuboid: Cuboid, kind: ColliderKind) -> ColliderHandle {
    colliders.insert(
        ColliderBuilder::cuboid(cuboid.half_extents.x, cuboid.half_extents.y, cuboid.half_extents.z)
            .position(Pose::translation(cuboid.center.x, cuboid.center.y, cuboid.center.z))
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
