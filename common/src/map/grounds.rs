use std::f32::consts::TAU;

use bevy_math::{EulerRot, IVec2, Quat, Vec2, Vec3};
use bincode::{Decode, Encode};
use serde::Deserialize;

use super::rocks::{ROCK_VARIANTS, RockClass};

const HILL_BLEND_START: f32 = 6.0;
const HILL_BLEND_END: f32 = 50.0;
// The terrain mesh: fine rings around the map, where the play is, and coarse
// ones out to the horizon. Collision and rendering share the same triangles.
const FINE_RING_EXTENT: f32 = 400.0;
const FINE_RING_SPACING: f32 = 4.0;
const DISTANT_RINGS: usize = 36;
const DISTANT_RING_SPACING: f32 = 25.0;
// Trees scatter over a jittered grid, so their number follows the ground's
// area, with a slow noise carving groves and clearings. They stop short of
// the map edge, which keeps the seam walkable, and end where the fog hides
// them.
const TREE_CELL: f32 = 16.0;
const TREE_CLEARANCE: f32 = 8.0;
const DECORATION_EXTENT: f32 = 700.0;
// Pebbles are too small to see from afar and too many to keep everywhere,
// so they stop this far past the map edge.
const PEBBLE_EXTENT: f32 = 160.0;
// How far into the ground a rock sits, as a fraction of its size.
const ROCK_SINK: f32 = 0.1;

#[derive(Debug, Clone, Deserialize, Encode, Decode)]
pub struct GroundsSettings {
    pub level: u8,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct Grounds {
    pub center: [f32; 2],
    pub half_size: [f32; 2],
    pub y: f32,
    pub settings: GroundsSettings,
}

pub struct GroundsMesh {
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<[u32; 3]>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DecorationKind {
    Tree,
    Rock(RockClass),
}

#[derive(Debug, Clone, Copy)]
pub struct GroundDecoration {
    pub position: Vec3,
    pub scale: Vec3,
    pub rotation: Quat,
    pub variant: u32,
    pub kind: DecorationKind,
}

impl GroundDecoration {
    pub fn collides(&self) -> bool {
        match self.kind {
            DecorationKind::Tree => true,
            DecorationKind::Rock(class) => class.collides(),
        }
    }
}

fn cell_hash(x: i32, z: i32, salt: u32) -> f32 {
    let mut value = (x as u32)
        .wrapping_mul(374761393)
        .wrapping_add((z as u32).wrapping_mul(668265263))
        .wrapping_add(salt.wrapping_mul(2246822519));
    value = (value ^ (value >> 13)).wrapping_mul(1274126177);
    (value ^ (value >> 16)) as f32 / u32::MAX as f32
}

fn value_noise(position: Vec2, salt: u32) -> f32 {
    let cell = position.floor().as_ivec2();
    let t = position - position.floor();
    let t = t * t * (Vec2::splat(3.0) - t * 2.0);
    let corner = |dx: i32, dz: i32| cell_hash(cell.x + dx, cell.y + dz, salt);
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let bottom = lerp(corner(0, 0), corner(1, 0), t.x);
    let top = lerp(corner(0, 1), corner(1, 1), t.x);
    lerp(bottom, top, t.y)
}

// Rock scatter per class: grid cell and clearance from the map seam in
// metres, the hash salt, and how the rockiness noise fills a cell.
fn rock_cell(class: RockClass) -> f32 {
    match class {
        RockClass::Pebble => 3.5,
        RockClass::Stone => 9.0,
        RockClass::Boulder => 40.0,
    }
}

fn rock_extent(class: RockClass) -> f32 {
    match class {
        RockClass::Pebble => PEBBLE_EXTENT,
        RockClass::Stone | RockClass::Boulder => DECORATION_EXTENT,
    }
}

fn rock_clearance(class: RockClass) -> f32 {
    match class {
        RockClass::Pebble => 1.5,
        RockClass::Stone => 4.0,
        RockClass::Boulder => 8.0,
    }
}

fn rock_salt(class: RockClass) -> u32 {
    match class {
        RockClass::Pebble => 20,
        RockClass::Stone => 40,
        RockClass::Boulder => 60,
    }
}

fn rock_density(class: RockClass, rockiness: f32) -> f32 {
    match class {
        RockClass::Pebble => 0.15 + 0.6 * smoothstep(0.3, 0.7, rockiness),
        RockClass::Stone => 0.05 + 0.6 * smoothstep(0.4, 0.75, rockiness),
        RockClass::Boulder => 0.06 + 0.6 * smoothstep(0.45, 0.8, rockiness),
    }
}

fn smoothstep(low: f32, high: f32, value: f32) -> f32 {
    let t = ((value - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Grounds {
    pub fn decorations(&self) -> Vec<GroundDecoration> {
        let mut decorations = Vec::new();
        let [low, high] = self.decoration_cells(TREE_CELL, DECORATION_EXTENT);
        for cz in low.y..high.y {
            for cx in low.x..high.x {
                decorations.extend(self.tree_at(cx, cz));
            }
        }
        for class in RockClass::ALL {
            let [low, high] = self.decoration_cells(rock_cell(class), rock_extent(class));
            for cz in low.y..high.y {
                for cx in low.x..high.x {
                    decorations.extend(self.rock_at(class, cx, cz));
                }
            }
        }
        decorations
    }

    pub fn collidable_decorations(&self) -> Vec<GroundDecoration> {
        self.decorations()
            .into_iter()
            .filter(GroundDecoration::collides)
            .collect()
    }

    // The rocks whose footprints may reach into the rectangle, so grass can
    // keep off them without walking the whole scatter.
    pub fn rocks_within(&self, min: Vec2, max: Vec2) -> Vec<GroundDecoration> {
        let mut rocks = Vec::new();
        for class in RockClass::ALL {
            let cell = rock_cell(class);
            let reach = class.size_range().1 * 2.0;
            let low = ((min - Vec2::splat(reach)) / cell).floor().as_ivec2();
            let high = ((max + Vec2::splat(reach)) / cell).floor().as_ivec2();
            for cz in low.y..=high.y {
                for cx in low.x..=high.x {
                    rocks.extend(self.rock_at(class, cx, cz));
                }
            }
        }
        rocks
    }

    fn rock_at(&self, class: RockClass, cx: i32, cz: i32) -> Option<GroundDecoration> {
        let cell = rock_cell(class);
        let salt = rock_salt(class);
        let x = (cx as f32 + 0.1 + 0.8 * cell_hash(cx, cz, salt)) * cell;
        let z = (cz as f32 + 0.1 + 0.8 * cell_hash(cx, cz, salt + 1)) * cell;
        let outside = self.distance_outside_footprint(x, z);
        if !(rock_clearance(class)..=rock_extent(class)).contains(&outside) {
            return None;
        }
        let rockiness = value_noise(Vec2::new(x, z) / 60.0, 11);
        if cell_hash(cx, cz, salt + 2) > rock_density(class, rockiness) {
            return None;
        }
        let (min, max) = class.size_range();
        let spread = cell_hash(cx, cz, salt + 3);
        let size = min + (max - min) * spread * spread;
        if self.tree_within(x, z, size + 1.0) {
            return None;
        }
        let settle = Quat::from_rotation_arc(Vec3::Y, self.normal(x, z).normalize_or(Vec3::Y));
        let yaw = Quat::from_rotation_y(cell_hash(cx, cz, salt + 4) * TAU);
        let tilt = Quat::from_euler(
            EulerRot::XYZ,
            (cell_hash(cx, cz, salt + 5) - 0.5) * 0.4,
            0.0,
            (cell_hash(cx, cz, salt + 6) - 0.5) * 0.4,
        );
        Some(GroundDecoration {
            position: Vec3::new(x, self.height(x, z) - size * ROCK_SINK, z),
            scale: Vec3::splat(size),
            rotation: settle * yaw * tilt,
            variant: (cell_hash(cx, cz, salt + 7) * ROCK_VARIANTS as f32) as u32,
            kind: DecorationKind::Rock(class),
        })
    }

    fn decoration_cells(&self, cell: f32, extent: f32) -> [IVec2; 2] {
        let center = Vec2::from(self.center);
        let reach = Vec2::from(self.half_size) + Vec2::splat(extent);
        [
            ((center - reach) / cell).floor().as_ivec2(),
            ((center + reach) / cell).ceil().as_ivec2(),
        ]
    }

    fn tree_at(&self, cx: i32, cz: i32) -> Option<GroundDecoration> {
        let x = (cx as f32 + 0.15 + 0.7 * cell_hash(cx, cz, 1)) * TREE_CELL;
        let z = (cz as f32 + 0.15 + 0.7 * cell_hash(cx, cz, 2)) * TREE_CELL;
        let outside = self.distance_outside_footprint(x, z);
        if !(TREE_CLEARANCE..=DECORATION_EXTENT).contains(&outside) {
            return None;
        }
        let grove = value_noise(Vec2::new(cx as f32, cz as f32) / 6.0, 9);
        let density = 0.1 + 0.9 * smoothstep(0.3, 0.7, grove);
        if cell_hash(cx, cz, 3) > density {
            return None;
        }
        let size = 0.8 + cell_hash(cx, cz, 5) * 0.8;
        Some(GroundDecoration {
            position: Vec3::new(x, self.height(x, z) - 0.2, z),
            scale: Vec3::splat(size),
            rotation: Quat::from_rotation_y(cell_hash(cx, cz, 6) * TAU),
            variant: (cell_hash(cx, cz, 7) * 3.0) as u32,
            kind: DecorationKind::Tree,
        })
    }

    fn tree_within(&self, x: f32, z: f32, distance: f32) -> bool {
        let cx = (x / TREE_CELL).floor() as i32;
        let cz = (z / TREE_CELL).floor() as i32;
        for dz in -1..=1 {
            for dx in -1..=1 {
                if let Some(tree) = self.tree_at(cx + dx, cz + dz)
                    && Vec2::new(tree.position.x - x, tree.position.z - z).length() < distance
                {
                    return true;
                }
            }
        }
        false
    }

    pub fn distance_outside_footprint(&self, x: f32, z: f32) -> f32 {
        ((x - self.center[0]).abs() - self.half_size[0]).max((z - self.center[1]).abs() - self.half_size[1])
    }

    // How far past the map edge the terrain reaches.
    pub fn extent(&self) -> f32 {
        FINE_RING_EXTENT + DISTANT_RINGS as f32 * DISTANT_RING_SPACING
    }

    pub fn height(&self, x: f32, z: f32) -> f32 {
        let distance = self.distance_outside_footprint(x, z).max(0.0);
        // Keep the map seam and its immediate surroundings level, then ease
        // into broad, low-gradient hills. Authored terrain slabs never call
        // this function and therefore remain perfectly flat.
        let blend = ((distance - HILL_BLEND_START) / (HILL_BLEND_END - HILL_BLEND_START)).clamp(0.0, 1.0);
        let blend = blend * blend * (3.0 - 2.0 * blend);
        let hills = (x * 0.018 + z * 0.006 + 0.6).sin() * 4.5
            + (x * 0.009 - z * 0.015 + 1.7).sin() * 3.0
            + (x * 0.043 + z * 0.031 - 0.4).sin() * 1.4;
        let distant = ((distance - 180.0) / 240.0).clamp(0.0, 1.0);
        self.y + blend * hills + distant * (18.0 + 16.0 * (x * 0.006 + z * 0.004).sin().powi(2))
    }

    // Surface normal of `height` from central differences; the same value on
    // every side of the mesh, so the seams where the four sides meet are lit
    // continuously.
    pub fn normal(&self, x: f32, z: f32) -> Vec3 {
        const STEP: f32 = 0.5;
        let dx = self.height(x + STEP, z) - self.height(x - STEP, z);
        let dz = self.height(x, z + STEP) - self.height(x, z - STEP);
        Vec3::new(-dx, 2.0 * STEP, -dz).normalize()
    }

    pub fn mesh(&self) -> GroundsMesh {
        let rings = (FINE_RING_EXTENT / FINE_RING_SPACING) as usize;
        let distances: Vec<f32> = (0..=rings)
            .map(|i| i as f32 * FINE_RING_SPACING)
            .chain((1..=DISTANT_RINGS).map(|i| FINE_RING_EXTENT + i as f32 * DISTANT_RING_SPACING))
            .collect();
        let segments = 64usize;
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        for side in 0..4 {
            let offset = vertices.len() as u32;
            for &distance in &distances {
                let hx = self.half_size[0] + distance;
                let hz = self.half_size[1] + distance;
                for step in 0..=segments {
                    let t = step as f32 / segments as f32 * 2.0 - 1.0;
                    let (x, z) = match side {
                        0 => (hx, t * hz),
                        1 => (-t * hx, hz),
                        2 => (-hx, -t * hz),
                        _ => (t * hx, -hz),
                    };
                    let (x, z) = (x + self.center[0], z + self.center[1]);
                    vertices.push(Vec3::new(x, self.height(x, z), z));
                }
            }
            for ring in 0..distances.len() - 1 {
                for step in 0..segments {
                    let a = offset + (ring * (segments + 1) + step) as u32;
                    let b = a + (segments + 1) as u32;
                    // An empty base level has no cutout: the innermost ring
                    // collapses to a point and needs a fan, not zero-area faces.
                    if ring != 0 || self.half_size != [0.0, 0.0] {
                        triangles.push([a, a + 1, b]);
                    }
                    triangles.push([a + 1, b + 1, b]);
                }
            }
        }
        GroundsMesh { vertices, triangles }
    }
}

#[cfg(test)]
#[path = "tests/grounds.rs"]
mod tests;
