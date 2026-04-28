use std::collections::VecDeque;

use super::edges::{CellSide, has_edge_on_cell_side};
use crate::{
    constants::{EXTERIOR_LIGHT_SEED, EXTERIOR_LIGHT_STEP_RETENTION, WALL_LIGHT_EXPOSURE_THRESHOLD, WALL_LIGHT_HEIGHT},
    resources::{CellGrid, EdgeGrid, LevelGrid},
};
use common::{
    constants::{FIELD_DEPTH, FIELD_WIDTH, GRID_SIZE, LEVEL_HEIGHT, WALL_THICKNESS},
    protocol::{Position, WallLight},
};

const MODEL_INSET: f32 = WALL_THICKNESS / 2.0 + 0.02;
const EXPOSURE_EPSILON: f32 = 0.001;
const CARDINAL_DIRECTIONS: [(CellSide, i32, i32); 4] = [
    (CellSide::North, -1, 0),
    (CellSide::South, 1, 0),
    (CellSide::West, 0, -1),
    (CellSide::East, 0, 1),
];

#[must_use]
pub fn generate_wall_lights(levels: &[LevelGrid], level_idx: usize) -> Vec<WallLight> {
    generate_wall_lights_from_parts(levels, level_idx)
}

fn generate_wall_lights_from_parts(levels: &[LevelGrid], level_idx: usize) -> Vec<WallLight> {
    let level_grid = &levels[level_idx];
    let cells = &level_grid.cells;
    let edges = &level_grid.edges;
    let exposure = compute_exterior_light_exposure(levels);
    let light_y = (level_idx as f32).mul_add(LEVEL_HEIGHT, WALL_LIGHT_HEIGHT);

    let mut lights = Vec::new();
    for row in 0..grid_rows(cells) {
        for col in 0..grid_cols(cells) {
            if !cell_can_have_wall_light(cells, row, col)
                || exposure[level_idx][row as usize][col as usize] >= WALL_LIGHT_EXPOSURE_THRESHOLD
            {
                continue;
            }

            let cell_center_x = (col as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
            let cell_center_z = (row as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));
            let half = GRID_SIZE / 2.0;

            if has_edge_on_cell_side(edges, row, col, CellSide::North) {
                lights.push(WallLight {
                    pos: Position {
                        x: cell_center_x,
                        y: light_y,
                        z: cell_center_z - half + MODEL_INSET,
                    },
                    yaw: 0.0,
                });
            }
            if has_edge_on_cell_side(edges, row, col, CellSide::South) {
                lights.push(WallLight {
                    pos: Position {
                        x: cell_center_x,
                        y: light_y,
                        z: cell_center_z + half - MODEL_INSET,
                    },
                    yaw: std::f32::consts::PI,
                });
            }
            if has_edge_on_cell_side(edges, row, col, CellSide::West) {
                lights.push(WallLight {
                    pos: Position {
                        x: cell_center_x - half + MODEL_INSET,
                        y: light_y,
                        z: cell_center_z,
                    },
                    yaw: std::f32::consts::FRAC_PI_2,
                });
            }
            if has_edge_on_cell_side(edges, row, col, CellSide::East) {
                lights.push(WallLight {
                    pos: Position {
                        x: cell_center_x + half - MODEL_INSET,
                        y: light_y,
                        z: cell_center_z,
                    },
                    yaw: -std::f32::consts::FRAC_PI_2,
                });
            }
        }
    }

    lights
}

fn compute_exterior_light_exposure(levels: &[LevelGrid]) -> Vec<Vec<Vec<f32>>> {
    let mut exposure = levels
        .iter()
        .map(|level| vec![vec![0.0; grid_cols(&level.cells) as usize]; grid_rows(&level.cells) as usize])
        .collect::<Vec<_>>();
    let mut queue = VecDeque::new();

    seed_top_sky(levels, &mut exposure, &mut queue);
    seed_boundary_openings(levels, &mut exposure, &mut queue);

    while let Some((level_idx, row, col)) = queue.pop_front() {
        let next_exposure = exposure[level_idx][row as usize][col as usize] * EXTERIOR_LIGHT_STEP_RETENTION;
        if next_exposure <= EXPOSURE_EPSILON {
            continue;
        }

        for (side, row_delta, col_delta) in CARDINAL_DIRECTIONS {
            let next_row = row + row_delta;
            let next_col = col + col_delta;
            if has_edge_on_cell_side(&levels[level_idx].edges, row, col, side)
                || !cell_in_bounds(&levels[level_idx].cells, next_row, next_col)
            {
                continue;
            }
            update_exposure(&mut exposure, &mut queue, level_idx, next_row, next_col, next_exposure);
        }

        if level_idx > 0 && column_is_open_between(levels, level_idx - 1, row, col) {
            update_exposure(&mut exposure, &mut queue, level_idx - 1, row, col, next_exposure);
        }
        if level_idx + 1 < levels.len() && column_is_open_between(levels, level_idx, row, col) {
            update_exposure(&mut exposure, &mut queue, level_idx + 1, row, col, next_exposure);
        }
    }

    exposure
}

fn seed_top_sky(levels: &[LevelGrid], exposure: &mut [Vec<Vec<f32>>], queue: &mut VecDeque<(usize, i32, i32)>) {
    let Some((top_level_idx, top_level)) = levels.len().checked_sub(1).map(|idx| (idx, &levels[idx])) else {
        return;
    };

    for row in 0..grid_rows(&top_level.cells) {
        for col in 0..grid_cols(&top_level.cells) {
            update_exposure(exposure, queue, top_level_idx, row, col, EXTERIOR_LIGHT_SEED);
        }
    }
}

fn seed_boundary_openings(
    levels: &[LevelGrid],
    exposure: &mut [Vec<Vec<f32>>],
    queue: &mut VecDeque<(usize, i32, i32)>,
) {
    for (level_idx, level_grid) in levels.iter().enumerate() {
        for row in 0..grid_rows(&level_grid.cells) {
            for col in 0..grid_cols(&level_grid.cells) {
                if cell_has_boundary_opening(&level_grid.cells, &level_grid.edges, row, col) {
                    update_exposure(exposure, queue, level_idx, row, col, EXTERIOR_LIGHT_SEED);
                }
            }
        }
    }
}

fn cell_has_boundary_opening(cells: &CellGrid, edges: &EdgeGrid, row: i32, col: i32) -> bool {
    CARDINAL_DIRECTIONS.iter().any(|(side, row_delta, col_delta)| {
        !has_edge_on_cell_side(edges, row, col, *side) && !cell_in_bounds(cells, row + row_delta, col + col_delta)
    })
}

fn update_exposure(
    exposure: &mut [Vec<Vec<f32>>],
    queue: &mut VecDeque<(usize, i32, i32)>,
    level_idx: usize,
    row: i32,
    col: i32,
    value: f32,
) {
    let current = &mut exposure[level_idx][row as usize][col as usize];
    if value > *current + EXPOSURE_EPSILON {
        *current = value;
        queue.push_back((level_idx, row, col));
    }
}

fn column_is_open_between(levels: &[LevelGrid], lower_level_idx: usize, row: i32, col: i32) -> bool {
    !cell_has_floor_slab(&levels[lower_level_idx + 1].cells, row, col)
}

fn cell_can_have_wall_light(cells: &CellGrid, row: i32, col: i32) -> bool {
    if row < 0 || col < 0 {
        return false;
    }
    cells
        .rows
        .get(row as usize)
        .and_then(|grid_row| grid_row.get(col as usize))
        .is_some_and(|cell| !cell.has_ramp && (cell.has_floor || cell.has_ramp_from_below))
}

fn cell_has_floor_slab(cells: &CellGrid, row: i32, col: i32) -> bool {
    if row < 0 || col < 0 {
        return false;
    }
    cells
        .rows
        .get(row as usize)
        .and_then(|grid_row| grid_row.get(col as usize))
        .is_some_and(|cell| cell.has_floor_slab)
}

fn cell_in_bounds(cells: &CellGrid, row: i32, col: i32) -> bool {
    row >= 0
        && col >= 0
        && cells
            .rows
            .get(row as usize)
            .is_some_and(|grid_row| grid_row.get(col as usize).is_some())
}

fn grid_rows(cells: &CellGrid) -> i32 {
    cells.rows.len() as i32
}

fn grid_cols(cells: &CellGrid) -> i32 {
    cells.rows.first().map_or(0, Vec::len) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark_floor_rect(cells: &mut CellGrid, col_min: usize, row_min: usize, col_max: usize, row_max: usize) {
        for row in row_min..row_max {
            for col in col_min..col_max {
                cells.rows[row][col].has_floor = true;
                cells.rows[row][col].has_floor_slab = true;
            }
        }
    }

    fn level_grid(cells: CellGrid, edges: EdgeGrid) -> LevelGrid {
        LevelGrid { cells, edges }
    }

    fn enclosed_edges(cols: i32, rows: i32) -> EdgeGrid {
        let mut edges = EdgeGrid::new(cols, rows);
        for col in 0..cols {
            edges.horizontal[0][col as usize] = true;
            edges.horizontal[rows as usize][col as usize] = true;
        }
        for row in 0..rows {
            edges.vertical[row as usize][0] = true;
            edges.vertical[row as usize][cols as usize] = true;
        }
        edges
    }

    #[test]
    fn wall_lights_use_level_height_offset() {
        let mut cells = CellGrid::new(1, 1);
        mark_floor_rect(&mut cells, 0, 0, 1, 1);
        let mut ceiling = CellGrid::new(1, 1);
        mark_floor_rect(&mut ceiling, 0, 0, 1, 1);
        let levels = vec![
            level_grid(CellGrid::new(1, 1), EdgeGrid::new(1, 1)),
            level_grid(CellGrid::new(1, 1), EdgeGrid::new(1, 1)),
            level_grid(cells, enclosed_edges(1, 1)),
            level_grid(ceiling, EdgeGrid::new(1, 1)),
        ];

        let lights = generate_wall_lights_from_parts(&levels, 2);

        assert_eq!(lights.len(), 4);
        assert!(
            lights
                .iter()
                .all(|light| light.pos.y == 2.0_f32.mul_add(LEVEL_HEIGHT, WALL_LIGHT_HEIGHT))
        );
    }

    #[test]
    fn wall_lights_require_floor() {
        let levels = vec![level_grid(CellGrid::new(1, 1), enclosed_edges(1, 1))];

        let lights = generate_wall_lights_from_parts(&levels, 0);

        assert!(lights.is_empty());
    }

    #[test]
    fn enclosed_top_level_is_sky_lit() {
        let mut cells = CellGrid::new(1, 1);
        mark_floor_rect(&mut cells, 0, 0, 1, 1);
        let levels = vec![level_grid(cells, enclosed_edges(1, 1))];

        let lights = generate_wall_lights_from_parts(&levels, 0);

        assert!(lights.is_empty());
    }

    #[test]
    fn covered_enclosed_floor_gets_wall_lights() {
        let mut cells = CellGrid::new(1, 1);
        mark_floor_rect(&mut cells, 0, 0, 1, 1);
        let mut ceiling = CellGrid::new(1, 1);
        mark_floor_rect(&mut ceiling, 0, 0, 1, 1);
        let levels = vec![
            level_grid(cells, enclosed_edges(1, 1)),
            level_grid(ceiling, EdgeGrid::new(1, 1)),
        ];

        let lights = generate_wall_lights_from_parts(&levels, 0);

        assert_eq!(lights.len(), 4);
    }

    #[test]
    fn ramp_footprint_never_gets_wall_lights() {
        let mut cells = CellGrid::new(1, 1);
        cells.rows[0][0].has_ramp = true;
        let mut ceiling = CellGrid::new(1, 1);
        mark_floor_rect(&mut ceiling, 0, 0, 1, 1);
        let levels = vec![
            level_grid(cells, enclosed_edges(1, 1)),
            level_grid(ceiling, EdgeGrid::new(1, 1)),
        ];

        let lights = generate_wall_lights_from_parts(&levels, 0);

        assert!(lights.is_empty());
    }

    #[test]
    fn ramp_from_below_can_support_wall_lights_when_dark() {
        let mut cells = CellGrid::new(1, 1);
        cells.rows[0][0].has_ramp_from_below = true;
        let mut ceiling = CellGrid::new(1, 1);
        mark_floor_rect(&mut ceiling, 0, 0, 1, 1);
        let levels = vec![
            level_grid(cells, enclosed_edges(1, 1)),
            level_grid(ceiling, EdgeGrid::new(1, 1)),
        ];

        let lights = generate_wall_lights_from_parts(&levels, 0);

        assert_eq!(lights.len(), 4);
    }

    #[test]
    fn side_opening_suppresses_nearby_wall_lights() {
        let mut cells = CellGrid::new(1, 1);
        let mut edges = enclosed_edges(1, 1);
        mark_floor_rect(&mut cells, 0, 0, 1, 1);
        edges.vertical[0][0] = false;
        let mut ceiling = CellGrid::new(1, 1);
        mark_floor_rect(&mut ceiling, 0, 0, 1, 1);
        let levels = vec![level_grid(cells, edges), level_grid(ceiling, EdgeGrid::new(1, 1))];

        let lights = generate_wall_lights_from_parts(&levels, 0);

        assert!(lights.is_empty());
    }

    #[test]
    fn vertical_opening_exposure_falls_with_level_distance() {
        let mut cells = CellGrid::new(1, 1);
        mark_floor_rect(&mut cells, 0, 0, 1, 1);
        let one_open_level = vec![
            level_grid(cells.clone(), enclosed_edges(1, 1)),
            level_grid(CellGrid::new(1, 1), enclosed_edges(1, 1)),
        ];
        let two_open_levels = vec![
            level_grid(cells, enclosed_edges(1, 1)),
            level_grid(CellGrid::new(1, 1), enclosed_edges(1, 1)),
            level_grid(CellGrid::new(1, 1), enclosed_edges(1, 1)),
        ];

        let one_level_exposure = compute_exterior_light_exposure(&one_open_level)[0][0][0];
        let two_level_exposure = compute_exterior_light_exposure(&two_open_levels)[0][0][0];

        assert!((one_level_exposure - EXTERIOR_LIGHT_SEED * EXTERIOR_LIGHT_STEP_RETENTION).abs() < EXPOSURE_EPSILON);
        assert!(
            (two_level_exposure - EXTERIOR_LIGHT_SEED * EXTERIOR_LIGHT_STEP_RETENTION.powi(2)).abs() < EXPOSURE_EPSILON
        );
        assert!(two_level_exposure < one_level_exposure);
    }

    #[test]
    fn wall_blocks_horizontal_exposure() {
        let mut cells = CellGrid::new(2, 1);
        let mut edges = enclosed_edges(2, 1);
        mark_floor_rect(&mut cells, 0, 0, 2, 1);
        edges.vertical[0][0] = false;
        edges.vertical[0][1] = true;
        let mut ceiling = CellGrid::new(2, 1);
        mark_floor_rect(&mut ceiling, 0, 0, 2, 1);
        let levels = vec![level_grid(cells, edges), level_grid(ceiling, EdgeGrid::new(2, 1))];

        let exposure = compute_exterior_light_exposure(&levels);

        assert_eq!(exposure[0][0][0], EXTERIOR_LIGHT_SEED);
        assert_eq!(exposure[0][0][1], 0.0);
    }

    #[test]
    fn internal_wall_gets_lights_on_both_sides_when_dark() {
        let mut cells = CellGrid::new(2, 1);
        let mut edges = enclosed_edges(2, 1);
        mark_floor_rect(&mut cells, 0, 0, 2, 1);
        edges.vertical[0][1] = true;
        let mut ceiling = CellGrid::new(2, 1);
        mark_floor_rect(&mut ceiling, 0, 0, 2, 1);
        let levels = vec![level_grid(cells, edges), level_grid(ceiling, EdgeGrid::new(2, 1))];

        let lights = generate_wall_lights_from_parts(&levels, 0);

        assert_eq!(lights.len(), 8);
        assert!(lights.iter().any(|light| light.yaw == std::f32::consts::FRAC_PI_2));
        assert!(lights.iter().any(|light| light.yaw == -std::f32::consts::FRAC_PI_2));
    }
}
