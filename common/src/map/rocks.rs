use std::collections::HashMap;

use bevy_math::Vec3;

// Rocks come in three sizes with their own character: pebbles are rounded,
// stones chipped, boulders angular.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RockClass {
    Pebble,
    Stone,
    Boulder,
}

pub const ROCK_VARIANTS: u32 = 4;
// The shape colliders are hulled from; its vertices are a subset of every
// finer level's, so the hull matches what is drawn at any distance.
pub const ROCK_HULL_SUBDIVISIONS: u32 = 1;

impl RockClass {
    pub const ALL: [Self; 3] = [Self::Pebble, Self::Stone, Self::Boulder];

    // Radius range in metres.
    pub fn size_range(self) -> (f32, f32) {
        match self {
            Self::Pebble => (0.12, 0.4),
            Self::Stone => (0.45, 1.3),
            Self::Boulder => (1.5, 3.0),
        }
    }

    // Pebbles are walked over; the others are solid.
    pub fn collides(self) -> bool {
        self != Self::Pebble
    }

    fn cuts(self) -> u32 {
        match self {
            Self::Pebble => 1,
            Self::Stone => 3,
            Self::Boulder => 6,
        }
    }

    fn relief(self) -> f32 {
        match self {
            Self::Pebble => 0.12,
            Self::Stone => 0.22,
            Self::Boulder => 0.26,
        }
    }

    fn index(self) -> u32 {
        match self {
            Self::Pebble => 0,
            Self::Stone => 1,
            Self::Boulder => 2,
        }
    }
}

pub struct RockShape {
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<[u32; 3]>,
}

// A rock about one metre across: an icosphere pushed in and out by noise
// over its directions, chipped flat by a few random planes, and squashed.
// Every step is a function of the vertex direction and the variant alone, so
// the subdivision levels share one silhouette.
pub fn rock_shape(class: RockClass, variant: u32, subdivisions: u32) -> RockShape {
    let (mut vertices, triangles) = icosphere(subdivisions);
    let seed = class.index() * 64 + variant % ROCK_VARIANTS;
    let relief = class.relief();
    let offsets: [Vec3; 3] = [0, 1, 2].map(|octave| {
        Vec3::new(
            rock_hash(seed, octave * 3),
            rock_hash(seed, octave * 3 + 1),
            rock_hash(seed, octave * 3 + 2),
        ) * 100.0
    });
    for vertex in &mut vertices {
        let direction = *vertex;
        let radius = 1.0
            + relief * rock_noise(direction * 1.6 + offsets[0], seed)
            + relief * 0.5 * rock_noise(direction * 3.7 + offsets[1], seed)
            + relief * 0.25 * rock_noise(direction * 8.3 + offsets[2], seed);
        *vertex = direction * radius;
    }
    for cut in 0..class.cuts() {
        let salt = 10 + cut * 3;
        let normal = Vec3::new(
            rock_hash(seed, salt) - 0.5,
            rock_hash(seed, salt + 1) - 0.5,
            rock_hash(seed, salt + 2) - 0.5,
        )
        .normalize_or(Vec3::Y);
        let offset = 0.62 + 0.3 * rock_hash(seed, salt + 40);
        for vertex in &mut vertices {
            let depth = vertex.dot(normal) - offset;
            if depth > 0.0 {
                *vertex -= depth * normal;
            }
        }
    }
    let squash = Vec3::new(
        1.0 + 0.5 * rock_hash(seed, 30),
        0.62 + 0.33 * rock_hash(seed, 31),
        0.8 + 0.4 * rock_hash(seed, 32),
    );
    for vertex in &mut vertices {
        *vertex *= squash;
    }
    RockShape { vertices, triangles }
}

// Smooth noise in -1..1 over a unit lattice.
pub fn rock_noise(position: Vec3, seed: u32) -> f32 {
    let cell = position.floor();
    let t = position - cell;
    let t = t * t * (Vec3::splat(3.0) - t * 2.0);
    let cell = cell.as_ivec3();
    let corner = |dx: i32, dy: i32, dz: i32| lattice_hash(cell.x + dx, cell.y + dy, cell.z + dz, seed);
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = lerp(corner(0, 0, 0), corner(1, 0, 0), t.x);
    let x10 = lerp(corner(0, 1, 0), corner(1, 1, 0), t.x);
    let x01 = lerp(corner(0, 0, 1), corner(1, 0, 1), t.x);
    let x11 = lerp(corner(0, 1, 1), corner(1, 1, 1), t.x);
    let y0 = lerp(x00, x10, t.y);
    let y1 = lerp(x01, x11, t.y);
    lerp(y0, y1, t.z) * 2.0 - 1.0
}

fn rock_hash(seed: u32, salt: u32) -> f32 {
    lattice_hash(seed as i32, salt as i32, 7, 0x5bd1e995)
}

fn lattice_hash(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let mut value = (x as u32)
        .wrapping_mul(374761393)
        .wrapping_add((y as u32).wrapping_mul(668265263))
        .wrapping_add((z as u32).wrapping_mul(2246822519))
        .wrapping_add(seed.wrapping_mul(3266489917));
    value = (value ^ (value >> 13)).wrapping_mul(1274126177);
    (value ^ (value >> 16)) as f32 / u32::MAX as f32
}

// Unit icosphere; each subdivision keeps the previous level's vertices in
// place and adds the edge midpoints after them.
fn icosphere(subdivisions: u32) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let t = (1.0 + 5.0f32.sqrt()) / 2.0;
    let mut vertices: Vec<Vec3> = [
        (-1.0, t, 0.0),
        (1.0, t, 0.0),
        (-1.0, -t, 0.0),
        (1.0, -t, 0.0),
        (0.0, -1.0, t),
        (0.0, 1.0, t),
        (0.0, -1.0, -t),
        (0.0, 1.0, -t),
        (t, 0.0, -1.0),
        (t, 0.0, 1.0),
        (-t, 0.0, -1.0),
        (-t, 0.0, 1.0),
    ]
    .iter()
    .map(|&(x, y, z)| Vec3::new(x, y, z).normalize())
    .collect();
    let mut triangles: Vec<[u32; 3]> = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    for _ in 0..subdivisions {
        let mut midpoints: HashMap<(u32, u32), u32> = HashMap::new();
        let mut midpoint = |a: u32, b: u32, vertices: &mut Vec<Vec3>| {
            let key = (a.min(b), a.max(b));
            *midpoints.entry(key).or_insert_with(|| {
                vertices.push((vertices[a as usize] + vertices[b as usize]).normalize());
                vertices.len() as u32 - 1
            })
        };
        let mut next = Vec::with_capacity(triangles.len() * 4);
        for [a, b, c] in triangles {
            let ab = midpoint(a, b, &mut vertices);
            let bc = midpoint(b, c, &mut vertices);
            let ca = midpoint(c, a, &mut vertices);
            next.extend([[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]);
        }
        triangles = next;
    }
    (vertices, triangles)
}

#[cfg(test)]
#[path = "tests/rocks.rs"]
mod tests;
