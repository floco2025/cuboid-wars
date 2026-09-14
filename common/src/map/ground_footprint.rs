use std::collections::BTreeMap;

use bincode::{Decode, Encode};

#[derive(Debug, Clone, Copy, Default, Encode, Decode)]
pub(super) struct GroundRect {
    pub x1: f32,
    pub x2: f32,
    pub z1: f32,
    pub z2: f32,
}

impl GroundRect {
    fn new((x1, x2, z1, z2): (f32, f32, f32, f32)) -> Self {
        assert!([x1, x2, z1, z2].into_iter().all(f32::is_finite));
        // Signed zero must name one edge in both deduplication and total_cmp lookups.
        let [x1, x2, z1, z2] = [x1, x2, z1, z2].map(|v| if v == 0.0 { 0.0 } else { v });
        Self {
            x1: x1.min(x2),
            x2: x1.max(x2),
            z1: z1.min(z2),
            z2: z1.max(z2),
        }
    }

    pub fn distance(&self, x: f32, z: f32) -> f32 {
        (self.x1 - x).max(x - self.x2).max(self.z1 - z).max(z - self.z2)
    }
}

#[derive(Debug, Clone, Encode, Decode)]
pub(super) struct GroundFootprint {
    pub bounds: GroundRect,
    cutouts: Vec<GroundRect>,
    pub infill: Vec<GroundRect>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CellState {
    Empty,
    Blocked,
    Exterior,
}

impl GroundFootprint {
    // Flood only from outside the geometry: enclosed voids still belong to
    // the map. Compress at compiled slab/wall edges rather than rasterizing
    // cells, so trim, narrow passages and basement openings survive intact.
    pub fn new(rectangles: impl IntoIterator<Item = (f32, f32, f32, f32)>) -> Self {
        let rectangles: Vec<_> = rectangles
            .into_iter()
            .map(GroundRect::new)
            .filter(|r| r.x1 < r.x2 && r.z1 < r.z2)
            .collect();
        if rectangles.is_empty() {
            return Self {
                bounds: GroundRect::default(),
                cutouts: Vec::new(),
                infill: Vec::new(),
            };
        }
        let coordinates = |x: bool| {
            let mut values: Vec<_> = rectangles
                .iter()
                .flat_map(|r| if x { [r.x1, r.x2] } else { [r.z1, r.z2] })
                .collect();
            values.sort_unstable_by(f32::total_cmp);
            values.dedup();
            values
        };
        let xs = coordinates(true);
        let zs = coordinates(false);
        let cols = xs.len() - 1;
        let rows = zs.len() - 1;
        let bounds = GroundRect {
            x1: xs[0],
            x2: xs[cols],
            z1: zs[0],
            z2: zs[rows],
        };
        let mut cells = vec![CellState::Empty; cols * rows];
        let index = |values: &[f32], value: f32| {
            values
                .binary_search_by(|v| v.total_cmp(&value))
                .expect("footprint edge missing")
        };
        for rect in rectangles {
            let (left, right) = (index(&xs, rect.x1), index(&xs, rect.x2));
            for row in index(&zs, rect.z1)..index(&zs, rect.z2) {
                cells[row * cols + left..row * cols + right].fill(CellState::Blocked);
            }
        }
        let mut queue = Vec::new();
        for row in 0..rows {
            for col in 0..cols {
                let i = row * cols + col;
                if (row == 0 || row == rows - 1 || col == 0 || col == cols - 1) && cells[i] == CellState::Empty {
                    cells[i] = CellState::Exterior;
                    queue.push(i);
                }
            }
        }
        let mut cursor = 0;
        while cursor < queue.len() {
            let i = queue[cursor];
            cursor += 1;
            for neighbor in [
                (i % cols > 0).then(|| i - 1),
                (i % cols + 1 < cols).then(|| i + 1),
                (i >= cols).then(|| i - cols),
                (i + cols < cells.len()).then(|| i + cols),
            ]
            .into_iter()
            .flatten()
            {
                if cells[neighbor] == CellState::Empty {
                    cells[neighbor] = CellState::Exterior;
                    queue.push(neighbor);
                }
            }
        }
        Self {
            bounds,
            cutouts: merge_regions(&xs, &zs, &cells, false),
            infill: merge_regions(&xs, &zs, &cells, true),
        }
    }

    pub fn distance(&self, x: f32, z: f32) -> f32 {
        self.cutouts
            .iter()
            .map(|rect| rect.distance(x, z))
            .reduce(f32::min)
            .unwrap_or_else(|| self.bounds.distance(x, z))
    }

    pub fn contains(&self, x: f32, z: f32) -> bool {
        self.cutouts.iter().any(|rect| rect.distance(x, z) <= 0.0)
    }
}

fn merge_regions(xs: &[f32], zs: &[f32], cells: &[CellState], outside: bool) -> Vec<GroundRect> {
    let cols = xs.len() - 1;
    let mut rectangles: Vec<GroundRect> = Vec::new();
    let mut previous: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    for row in 0..zs.len() - 1 {
        let mut current = BTreeMap::new();
        let mut col = 0;
        while col < cols {
            if (cells[row * cols + col] == CellState::Exterior) != outside {
                col += 1;
                continue;
            }
            let start = col;
            while col < cols && (cells[row * cols + col] == CellState::Exterior) == outside {
                col += 1;
            }
            let index = if let Some(&index) = previous.get(&(start, col)) {
                rectangles[index].z2 = zs[row + 1];
                index
            } else {
                rectangles.push(GroundRect {
                    x1: xs[start],
                    x2: xs[col],
                    z1: zs[row],
                    z2: zs[row + 1],
                });
                rectangles.len() - 1
            };
            current.insert((start, col), index);
        }
        previous = current;
    }
    rectangles
}

#[cfg(test)]
#[path = "tests/ground_footprint.rs"]
mod tests;
