use bevy::prelude::*;
use rand::{RngExt, rng, rngs::ThreadRng, seq::IndexedRandom};

use crate::resources::{ActorSpawnZone, MapConfig, PlayerSpawnZone};
use common::{
    config::CharacterPhysicsConfig,
    constants::{GRID_CELL_SIZE, LEVEL_HEIGHT},
    map_geometry::MapGeometry,
    physics::{CollisionWorld, character_paths_intersect},
    protocol::Position,
};

const SPAWN_MAX_ATTEMPTS: usize = 100;

// Pick a random clear position from any player spawn zone. All cells across
// all player zones are pooled and one is picked uniformly at random; no per-
// zone capacity tracking, no fallback. Used by login and player fall recovery.
//
// Returns the world origin if no player zone has any spawnable cells.
#[must_use]
pub fn generate_player_spawn_position(
    map_config: &MapConfig,
    map_geometry: &MapGeometry,
    collision_world: &CollisionWorld,
    occupied_positions: &[Position],
    character_physics: CharacterPhysicsConfig,
) -> Position {
    let mut valid_cells = Vec::new();
    for zone in &map_config.player_spawn_zones {
        valid_cells.extend(valid_cells_in_player_zone(map_config, zone));
    }
    pick_clear_position(
        &valid_cells,
        map_geometry,
        collision_world,
        occupied_positions,
        character_physics,
        "player",
    )
}

// Pick a random clear position from a single actor spawn zone. Used by the
// actor quota spawner — when topping a specific zone up, we never want to
// spill into other zones.
#[must_use]
pub fn generate_actor_spawn_position_in_zone(
    map_config: &MapConfig,
    map_geometry: &MapGeometry,
    zone_index: usize,
    collision_world: &CollisionWorld,
    occupied_positions: &[Position],
    character_physics: CharacterPhysicsConfig,
) -> Position {
    let Some(zone) = map_config.actor_spawn_zones.get(zone_index) else {
        warn!("actor spawn zone index {zone_index} out of range; spawning at center");
        return Position::default();
    };
    let valid_cells = valid_cells_in_actor_zone(map_config, zone);
    pick_clear_position(
        &valid_cells,
        map_geometry,
        collision_world,
        occupied_positions,
        character_physics,
        "actor",
    )
}

fn valid_cells_in_actor_zone(map_config: &MapConfig, zone: &ActorSpawnZone) -> Vec<SpawnCell> {
    collect_valid_cells(map_config, zone.level, zone.cells())
}

fn valid_cells_in_player_zone(map_config: &MapConfig, zone: &PlayerSpawnZone) -> Vec<SpawnCell> {
    collect_valid_cells(map_config, zone.level, zone.cells())
}

// (level, col, row) — same axis order as the file format's `cols`/`rows`
// arrays and the editor's drag tool. Internally the cell grid is indexed
// `[row][col]`, but that's local to the bounds checks below.
type SpawnCell = (u8, i32, i32);

fn collect_valid_cells(map_config: &MapConfig, level: u8, cells: impl Iterator<Item = (i32, i32)>) -> Vec<SpawnCell> {
    let Some(level_grid) = map_config.levels.get(level as usize) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let grid_cells = &level_grid.cells.rows;
    for (col, row) in cells {
        if row < 0 || row >= grid_cells.len() as i32 {
            continue;
        }
        if col < 0 || col >= grid_cells[row as usize].len() as i32 {
            continue;
        }
        let cell = &grid_cells[row as usize][col as usize];
        if cell.is_spawnable() {
            out.push((level, col, row));
        }
    }
    out
}

fn pick_clear_position(
    valid_cells: &[SpawnCell],
    map_geometry: &MapGeometry,
    collision_world: &CollisionWorld,
    occupied_positions: &[Position],
    character_physics: CharacterPhysicsConfig,
    label: &str,
) -> Position {
    if valid_cells.is_empty() {
        warn!("no valid spawn cells for {label:?} (all obstructions or empty), spawning at center");
        return Position::default();
    }

    let mut rng = rng();
    for _ in 0..SPAWN_MAX_ATTEMPTS {
        let &(level, col, row) = valid_cells.choose(&mut rng).expect("valid_cells should not be empty");
        let pos = random_position_in_spawn_cell(&mut rng, map_geometry, level, col, row, character_physics);

        if character_spawn_position_is_clear(&pos, collision_world, occupied_positions, character_physics) {
            return pos;
        }
    }

    warn!(
        "could not generate spawn position for {label:?} after {} attempts, spawning at center",
        SPAWN_MAX_ATTEMPTS
    );
    Position::default()
}

fn random_position_in_spawn_cell(
    rng: &mut ThreadRng,
    geometry: &MapGeometry,
    level: u8,
    col: i32,
    row: i32,
    character_physics: CharacterPhysicsConfig,
) -> Position {
    let cell_min_x = geometry.cell_to_world_x(col);
    let cell_max_x = cell_min_x + GRID_CELL_SIZE;
    let cell_min_z = geometry.cell_to_world_z(row);
    let cell_max_z = cell_min_z + GRID_CELL_SIZE;

    Position {
        x: rng.random_range(
            (cell_min_x + character_physics.collider.width / 2.0)
                ..=(cell_max_x - character_physics.collider.width / 2.0),
        ),
        y: f32::from(level) * LEVEL_HEIGHT,
        z: rng.random_range(
            (cell_min_z + character_physics.collider.depth / 2.0)
                ..=(cell_max_z - character_physics.collider.depth / 2.0),
        ),
    }
}

fn character_spawn_position_is_clear(
    pos: &Position,
    collision_world: &CollisionWorld,
    occupied_positions: &[Position],
    character_physics: CharacterPhysicsConfig,
) -> bool {
    let character_center = Vec3::new(pos.x, character_physics.collider_center_y(pos.y), pos.z);
    let character_half_extents = Vec3::new(
        character_physics.collider.width / 2.0,
        character_physics.collision_height() / 2.0,
        character_physics.collider.depth / 2.0,
    );

    !occupied_positions
        .iter()
        .any(|other| character_position_intersects_character(pos, other, character_physics))
        && !collision_world.cuboid_overlaps_wall(character_center, character_half_extents)
}

fn character_position_intersects_character(
    pos: &Position,
    other: &Position,
    character_physics: CharacterPhysicsConfig,
) -> bool {
    character_paths_intersect(pos, pos, character_physics, other, other, character_physics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::{CellGrid, EdgeGrid, LevelGrid, MapConfig, PlayerSpawnZone};
    use common::{
        config::GameplayConfig,
        constants::WALL_THICKNESS,
        protocol::{MapLayout, Wall},
    };

    fn empty_layout() -> MapLayout {
        MapLayout::default()
    }

    fn collision_world(layout: &MapLayout) -> CollisionWorld {
        CollisionWorld::from_map_layout(layout, &common::protocol::BarrierKindTable::default())
    }

    fn character_physics() -> CharacterPhysicsConfig {
        GameplayConfig::load_default()
            .expect("default gameplay config should load")
            .player
            .physics()
    }

    fn map_config_with_player_spawn(level: u8, col: i32, row: i32) -> MapConfig {
        let mut levels = (0..=level)
            .map(|_| LevelGrid {
                cells: CellGrid::new(2, 2),
                edges: EdgeGrid::new(2, 2),
            })
            .collect::<Vec<_>>();
        levels[usize::from(level)].cells.rows[row as usize][col as usize].has_floor = true;
        MapConfig {
            levels,
            actor_spawn_zones: Vec::new(),
            player_spawn_zones: vec![PlayerSpawnZone {
                level,
                cols: [col, col + 1],
                rows: [row, row + 1],
            }],
            cookie_spawn_zones: Vec::new(),
            key_spawn_zones: Vec::new(),
            pressure_plates: Vec::new(),
        }
    }

    #[test]
    fn spawn_position_rejects_other_player_overlap() {
        let layout = empty_layout();
        let collision_world = collision_world(&layout);
        let pos = Position::default();

        assert!(!character_spawn_position_is_clear(
            &pos,
            &collision_world,
            &[pos],
            character_physics()
        ));
    }

    #[test]
    fn spawn_position_rejects_wall_overlap() {
        let mut layout = empty_layout();
        layout.walls.push(Wall {
            x1: -1.0,
            z1: 0.0,
            x2: 1.0,
            z2: 0.0,
            width: WALL_THICKNESS,
            level: 0,
        });
        let collision_world = collision_world(&layout);

        assert!(!character_spawn_position_is_clear(
            &Position::default(),
            &collision_world,
            &[],
            character_physics()
        ));
    }

    #[test]
    fn spawn_position_ignores_wall_on_other_level() {
        let mut layout = empty_layout();
        layout.walls.push(Wall {
            x1: -1.0,
            z1: 0.0,
            x2: 1.0,
            z2: 0.0,
            width: WALL_THICKNESS,
            level: 1,
        });
        let collision_world = collision_world(&layout);

        assert!(character_spawn_position_is_clear(
            &Position::default(),
            &collision_world,
            &[],
            character_physics()
        ));
    }

    #[test]
    fn player_spawn_position_uses_configured_spawn_level() {
        let layout = empty_layout();
        let collision_world = collision_world(&layout);
        let map_config = map_config_with_player_spawn(1, 0, 0);
        let geometry = MapGeometry::new(1, 1);

        let pos = generate_player_spawn_position(&map_config, &geometry, &collision_world, &[], character_physics());

        assert_eq!(pos.y, LEVEL_HEIGHT);
    }
}
