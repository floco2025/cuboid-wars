use crate::{
    constants::*,
    resources::{GridCell, GridConfig},
};
use common::{constants::*, protocol::Velocity};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GridDirection {
    None,
    East,
    North,
    West,
    South,
}

impl GridDirection {
    pub const ALL: [Self; 4] = [Self::East, Self::North, Self::West, Self::South];

    #[must_use]
    pub fn to_velocity(self) -> Velocity {
        match self {
            Self::None => Velocity {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            Self::East => Velocity {
                x: SENTRY_SPEED,
                y: 0.0,
                z: 0.0,
            },
            Self::North => Velocity {
                x: 0.0,
                y: 0.0,
                z: -SENTRY_SPEED,
            },
            Self::West => Velocity {
                x: -SENTRY_SPEED,
                y: 0.0,
                z: 0.0,
            },
            Self::South => Velocity {
                x: 0.0,
                y: 0.0,
                z: SENTRY_SPEED,
            },
        }
    }

    #[must_use]
    pub const fn opposite(self) -> Self {
        match self {
            Self::None => Self::None,
            Self::East => Self::West,
            Self::North => Self::South,
            Self::West => Self::East,
            Self::South => Self::North,
        }
    }

    #[must_use]
    pub const fn is_blocked(self, cell: GridCell) -> bool {
        match self {
            Self::None => false,
            Self::East => cell.has_east_wall,
            Self::North => cell.has_north_wall,
            Self::West => cell.has_west_wall,
            Self::South => cell.has_south_wall,
        }
    }
}

#[must_use]
pub fn direction_from_velocity(vel: &Velocity) -> GridDirection {
    if vel.x > 0.0 {
        GridDirection::East
    } else if vel.x < 0.0 {
        GridDirection::West
    } else if vel.z < 0.0 {
        GridDirection::North
    } else if vel.z > 0.0 {
        GridDirection::South
    } else {
        GridDirection::None
    }
}

#[must_use]
pub fn valid_directions(grid_config: &GridConfig, grid_x: i32, grid_z: i32, cell: GridCell) -> Vec<GridDirection> {
    assert!(
        (0..GRID_COLS).contains(&grid_x) && (0..GRID_ROWS).contains(&grid_z),
        "sentry current cell OOB in valid_directions: ({grid_x}, {grid_z})"
    );

    // Prefer non-ramp exits; we expect at least one exists for a non-ramp cell
    let open: Vec<_> = GridDirection::ALL
        .iter()
        .copied()
        .filter(|dir| !dir.is_blocked(cell))
        .collect();

    assert!(!open.is_empty(), "no open directions from grid cell");

    let ramp_safe: Vec<_> = open
        .iter()
        .copied()
        .filter(|dir| !direction_leads_to_ramp(grid_config, grid_x, grid_z, *dir))
        .collect();

    assert!(!ramp_safe.is_empty(), "all open directions lead to ramps");

    ramp_safe
}

#[must_use]
pub fn direction_leads_to_ramp(grid_config: &GridConfig, grid_x: i32, grid_z: i32, dir: GridDirection) -> bool {
    assert!(
        (0..GRID_COLS).contains(&grid_x) && (0..GRID_ROWS).contains(&grid_z),
        "sentry current cell OOB in direction_leads_to_ramp: ({grid_x}, {grid_z})"
    );

    let (next_x, next_z) = match dir {
        GridDirection::None => return false,
        GridDirection::East => (grid_x + 1, grid_z),
        GridDirection::North => (grid_x, grid_z - 1),
        GridDirection::West => (grid_x - 1, grid_z),
        GridDirection::South => (grid_x, grid_z + 1),
    };

    if !(0..GRID_COLS).contains(&next_x) || !(0..GRID_ROWS).contains(&next_z) {
        return true; // out-of-bounds neighbor is considered blocked
    }

    grid_config.grid[next_z as usize][next_x as usize].has_ramp
}

#[must_use]
pub fn forward_directions(valid: &[GridDirection], current: GridDirection) -> Vec<GridDirection> {
    valid.iter().copied().filter(|dir| *dir != current.opposite()).collect()
}

pub fn pick_direction<T: rand::Rng>(rng: &mut T, options: &[GridDirection]) -> Option<GridDirection> {
    if options.is_empty() {
        None
    } else {
        Some(options[rng.random_range(0..options.len())])
    }
}
