use crate::{
    constants::*,
    resources::GridConfig,
};
use common::{constants::*, protocol::{SentryId, Velocity}};

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
    pub fn is_blocked(
        self,
        grid_config: &GridConfig,
        grid_x: i32,
        grid_z: i32,
        sentry_grid: &[Vec<Option<SentryId>>],
        sentry_id: SentryId,
    ) -> bool {
        if self == Self::None {
            return false;
        }

        let cell = grid_config.grid[grid_z as usize][grid_x as usize];

        // Check walls
        let wall_blocked = match self {
            Self::None => false,
            Self::East => cell.has_east_wall,
            Self::North => cell.has_north_wall,
            Self::West => cell.has_west_wall,
            Self::South => cell.has_south_wall,
        };

        if wall_blocked {
            return true;
        }

        // Check if leads to ramp
        let (next_x, next_z) = match self {
            Self::North => (grid_x, grid_z - 1),
            Self::South => (grid_x, grid_z + 1),
            Self::East => (grid_x + 1, grid_z),
            Self::West => (grid_x - 1, grid_z),
            Self::None => (grid_x, grid_z),
        };

        if !(0..GRID_COLS).contains(&next_x) || !(0..GRID_ROWS).contains(&next_z) {
            return true; // out-of-bounds neighbor is considered blocked
        }

        if grid_config.grid[next_z as usize][next_x as usize].has_ramp {
            return true;
        }

        // Check if target cell is occupied by another sentry
        let cell_occupant = sentry_grid[next_z as usize][next_x as usize];
        if let Some(occupant) = cell_occupant {
            if occupant != sentry_id {
                return true; // Blocked by another sentry
            }
        }

        false
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
pub fn valid_directions(
    grid_config: &GridConfig,
    grid_x: i32,
    grid_z: i32,
    sentry_grid: &[Vec<Option<SentryId>>],
    sentry_id: SentryId,
) -> Vec<GridDirection> {
    assert!(
        (0..GRID_COLS).contains(&grid_x) && (0..GRID_ROWS).contains(&grid_z),
        "sentry current cell OOB in valid_directions: ({grid_x}, {grid_z})"
    );

    // Filter all directions using the unified is_blocked check
    let valid: Vec<_> = GridDirection::ALL
        .iter()
        .copied()
        .filter(|dir| !dir.is_blocked(grid_config, grid_x, grid_z, sentry_grid, sentry_id))
        .collect();

    valid
}

#[must_use]
pub fn ahead_directions(valid: &[GridDirection], current: GridDirection) -> Vec<GridDirection> {
    valid.iter().copied().filter(|dir| *dir != current.opposite()).collect()
}

pub fn pick_direction<T: rand::Rng>(rng: &mut T, options: &[GridDirection]) -> Option<GridDirection> {
    if options.is_empty() {
        None
    } else {
        Some(options[rng.random_range(0..options.len())])
    }
}
