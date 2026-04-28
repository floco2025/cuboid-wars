use bevy_ecs::prelude::Resource;
use bevy_math::Vec3;

use super::{Cuboid, floor_cuboid, wall_cuboid};
use crate::{
    constants::{PHYSICS_EPSILON, PLAYER_LANDING_EPSILON},
    map::{RampAxis, ramp_axis},
    protocol::{Floor, MapLayout, Ramp, Wall},
};

#[derive(Debug, Clone, PartialEq, Resource)]
pub struct CollisionWorld {
    pub solids: Vec<CollisionSolid>,
    pub supports: Vec<SupportSurface>,
}

impl CollisionWorld {
    #[must_use]
    pub fn from_map_layout(map_layout: &MapLayout) -> Self {
        let solids = map_layout
            .walls
            .iter()
            .map(wall_solid)
            .chain(map_layout.floors.iter().map(floor_solid))
            .chain(map_layout.ramps.iter().map(ramp_solid))
            .collect();

        let supports = map_layout
            .floors
            .iter()
            .map(floor_support)
            .chain(map_layout.ramps.iter().map(ramp_support))
            .collect();

        Self { solids, supports }
    }

    #[must_use]
    pub fn find_support(&self, x: f32, z: f32, y: f32) -> Option<f32> {
        let lo = y - PLAYER_LANDING_EPSILON;
        let hi = y + PLAYER_LANDING_EPSILON;

        self.supports
            .iter()
            .filter_map(|support| support.surface_y_at(x, z))
            .filter(|support_y| *support_y >= lo && *support_y <= hi)
            .max_by(f32::total_cmp)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionSolid {
    pub shape: CollisionShape,
    pub phasing_passthrough: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CollisionShape {
    Cuboid(Cuboid),
    Wedge(Wedge),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SupportSurface {
    Flat(FlatSupport),
    Sloped(SlopedSupport),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlatSupport {
    pub footprint: Rect,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlopedSupport {
    pub wedge: Wedge,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wedge {
    pub bounds: Bounds3,
    pub slope_axis: Axis,
    pub low_at: f32,
    pub high_at: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Z,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub min_x: f32,
    pub max_x: f32,
    pub min_z: f32,
    pub max_z: f32,
}

impl Rect {
    #[must_use]
    pub fn contains(&self, x: f32, z: f32) -> bool {
        x >= self.min_x && x <= self.max_x && z >= self.min_z && z <= self.max_z
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds3 {
    pub center: Vec3,
    pub half_extents: Vec3,
}

impl Bounds3 {
    #[must_use]
    pub fn min_y(&self) -> f32 {
        self.center.y - self.half_extents.y
    }

    #[must_use]
    pub fn max_y(&self) -> f32 {
        self.center.y + self.half_extents.y
    }

    #[must_use]
    pub fn footprint(&self) -> Rect {
        Rect {
            min_x: self.center.x - self.half_extents.x,
            max_x: self.center.x + self.half_extents.x,
            min_z: self.center.z - self.half_extents.z,
            max_z: self.center.z + self.half_extents.z,
        }
    }
}

impl SupportSurface {
    #[must_use]
    pub fn surface_y_at(&self, x: f32, z: f32) -> Option<f32> {
        match self {
            Self::Flat(flat) => flat.surface_y_at(x, z),
            Self::Sloped(sloped) => sloped.surface_y_at(x, z),
        }
    }
}

impl FlatSupport {
    #[must_use]
    pub fn surface_y_at(&self, x: f32, z: f32) -> Option<f32> {
        self.footprint.contains(x, z).then_some(self.y)
    }
}

impl SlopedSupport {
    #[must_use]
    pub fn surface_y_at(&self, x: f32, z: f32) -> Option<f32> {
        self.wedge.surface_y_at(x, z)
    }
}

impl Wedge {
    #[must_use]
    pub fn surface_y_at(&self, x: f32, z: f32) -> Option<f32> {
        if !self.bounds.footprint().contains(x, z) {
            return None;
        }

        let coord = match self.slope_axis {
            Axis::X => x,
            Axis::Z => z,
        };
        let denom = self.high_at - self.low_at;
        let progress = if denom.abs() < PHYSICS_EPSILON {
            0.0
        } else {
            ((coord - self.low_at) / denom).clamp(0.0, 1.0)
        };

        Some(self.bounds.min_y() + progress * (self.bounds.max_y() - self.bounds.min_y()))
    }
}

impl From<Cuboid> for Bounds3 {
    fn from(cuboid: Cuboid) -> Self {
        Self {
            center: cuboid.center,
            half_extents: cuboid.half_extents,
        }
    }
}

impl From<RampAxis> for Axis {
    fn from(axis: RampAxis) -> Self {
        match axis {
            RampAxis::X => Self::X,
            RampAxis::Z => Self::Z,
        }
    }
}

fn wall_solid(wall: &Wall) -> CollisionSolid {
    CollisionSolid {
        shape: CollisionShape::Cuboid(wall_cuboid(wall, 0.0)),
        phasing_passthrough: true,
    }
}

fn floor_solid(floor: &Floor) -> CollisionSolid {
    CollisionSolid {
        shape: CollisionShape::Cuboid(floor_cuboid(floor, 0.0)),
        phasing_passthrough: false,
    }
}

fn ramp_solid(ramp: &Ramp) -> CollisionSolid {
    CollisionSolid {
        shape: CollisionShape::Wedge(wedge_from_ramp(ramp)),
        phasing_passthrough: false,
    }
}

fn floor_support(floor: &Floor) -> SupportSurface {
    SupportSurface::Flat(FlatSupport {
        footprint: rect_from_bounds_xz(floor.bounds_xz()),
        y: floor.y,
    })
}

fn ramp_support(ramp: &Ramp) -> SupportSurface {
    SupportSurface::Sloped(SlopedSupport {
        wedge: wedge_from_ramp(ramp),
    })
}

fn wedge_from_ramp(ramp: &Ramp) -> Wedge {
    let (min_x, max_x, min_z, max_z) = ramp.bounds_xz();
    let (min_y, max_y) = ramp.bounds_y();
    let slope_axis = Axis::from(ramp_axis(ramp));
    let high_is_second = ramp.y2 >= ramp.y1;
    let (low_at, high_at) = match slope_axis {
        Axis::X => {
            let low = if high_is_second { ramp.x1 } else { ramp.x2 };
            let high = if high_is_second { ramp.x2 } else { ramp.x1 };
            (low, high)
        }
        Axis::Z => {
            let low = if high_is_second { ramp.z1 } else { ramp.z2 };
            let high = if high_is_second { ramp.z2 } else { ramp.z1 };
            (low, high)
        }
    };

    Wedge {
        bounds: Bounds3 {
            center: Vec3::new(
                f32::midpoint(min_x, max_x),
                f32::midpoint(min_y, max_y),
                f32::midpoint(min_z, max_z),
            ),
            half_extents: Vec3::new((max_x - min_x) / 2.0, (max_y - min_y) / 2.0, (max_z - min_z) / 2.0),
        },
        slope_axis,
        low_at,
        high_at,
    }
}

fn rect_from_bounds_xz((min_x, max_x, min_z, max_z): (f32, f32, f32, f32)) -> Rect {
    Rect {
        min_x,
        max_x,
        min_z,
        max_z,
    }
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

        assert_eq!(world.solids.len(), 3);
        assert!(matches!(world.solids[0].shape, CollisionShape::Cuboid(_)));
        assert!(matches!(world.solids[1].shape, CollisionShape::Cuboid(_)));
        assert!(matches!(world.solids[2].shape, CollisionShape::Wedge(_)));
    }

    #[test]
    fn collision_world_contains_supports_for_floors_and_ramps() {
        let world = CollisionWorld::from_map_layout(&test_map_layout());

        assert_eq!(world.supports.len(), 2);
        assert!(matches!(world.supports[0], SupportSurface::Flat(_)));
        assert!(matches!(world.supports[1], SupportSurface::Sloped(_)));
    }

    #[test]
    fn wall_solid_uses_wall_level_height() {
        let world = CollisionWorld::from_map_layout(&test_map_layout());
        let CollisionShape::Cuboid(cuboid) = world.solids[0].shape else {
            panic!("expected wall cuboid");
        };

        assert_eq!(cuboid.center.y, LEVEL_HEIGHT + WALL_HEIGHT / 2.0);
    }

    #[test]
    fn ramp_converts_to_wedge_with_slope_axis() {
        let world = CollisionWorld::from_map_layout(&test_map_layout());
        let CollisionShape::Wedge(wedge) = world.solids[2].shape else {
            panic!("expected ramp wedge");
        };

        assert_eq!(wedge.slope_axis, Axis::Z);
        assert_eq!(wedge.low_at, 0.0);
        assert_eq!(wedge.high_at, 8.0);
    }
}
