use bevy::prelude::*;
use rand::{
    RngExt, rng,
    rngs::ThreadRng,
    seq::{IndexedRandom, SliceRandom},
};

use crate::map::{ActorSpawnZone, CarrierGrid, MapConfig};
use common::{
    config::{ActorGameplayConfig, CharacterPhysicsConfig},
    map::{Carriers, MapGeometry},
    physics::{CollisionWorld, character_paths_intersect},
    protocol::{CarrierId, Position},
};

const SPAWN_MAX_ATTEMPTS: usize = 100;

// The first of up to `SPAWN_MAX_ATTEMPTS` candidates that `clear` accepts
// and no occupied body intersects. `candidate` gets the attempt number, so
// a caller can try a preferred spot first.
pub(crate) fn sample_clear_position(
    occupied_positions: &[Position],
    character_physics: CharacterPhysicsConfig,
    mut candidate: impl FnMut(usize, &mut ThreadRng) -> Option<Position>,
    clear: impl Fn(&Position) -> bool,
) -> Option<Position> {
    let mut rng = rng();
    (0..SPAWN_MAX_ATTEMPTS).find_map(|attempt| {
        let pos = candidate(attempt, &mut rng)?;
        (clear(&pos)
            && !occupied_positions
                .iter()
                .any(|other| character_position_intersects_character(&pos, other, character_physics)))
        .then_some(pos)
    })
}

// Initial facing for a freshly spawned character: toward the map origin (0,0).
// The negation + atan2 argument order is the non-obvious part, so the rule
// lives in one place shared by login and respawn.
#[must_use]
pub fn spawn_face_yaw(pos: &Position) -> f32 {
    (-pos.x).atan2(-pos.z)
}

// Pick a random clear position from any player spawn zone, on the map or
// on a nested map. All cells across all player zones are pooled and one is
// picked uniformly at random; no per-zone capacity tracking, no fallback.
// Used by login and player fall recovery.
//
// Returns the world origin if no player zone has any spawnable cells.
#[must_use]
pub fn generate_player_spawn_position(
    map_config: &MapConfig,
    carriers: &Carriers,
    collision_world: &CollisionWorld,
    occupied_positions: &[Position],
    character_physics: CharacterPhysicsConfig,
) -> Position {
    let mut valid_cells = Vec::new();
    for zone in &map_config.player_spawn_zones {
        valid_cells.extend(collect_valid_cells(
            map_config.grid(zone.carrier),
            zone.level,
            zone.cells(),
        ));
    }
    pick_clear_position(
        &valid_cells,
        map_config,
        carriers,
        collision_world,
        occupied_positions,
        character_physics,
    )
    .unwrap_or_else(|| {
        warn!(
            "no clear player spawn position among {} spawnable cells, spawning at center",
            valid_cells.len()
        );
        Position::default()
    })
}

// Pick a clear position from a single actor spawn zone, on the zone's
// carrier. Used by the actor quota spawner — when topping a specific zone
// up, we never want to spill into other zones. `None` when the zone has no
// clear spot right now; the caller leaves the slot empty rather than spawn
// somewhere the actor does not belong.
#[must_use]
pub fn generate_actor_spawn_position_in_zone(
    map_config: &MapConfig,
    carriers: &Carriers,
    zone: &ActorSpawnZone,
    collision_world: &CollisionWorld,
    occupied_positions: &[Position],
    actor_config: &ActorGameplayConfig,
) -> Option<Position> {
    let character_physics = actor_config.physics();
    if actor_config.immovable {
        let grid = map_config.grid(zone.carrier);
        let mut cells: Vec<_> = zone.immovable_cells(grid).collect();
        cells.shuffle(&mut rng());
        return cells.into_iter().find_map(|(col, row)| {
            let local = Position {
                x: grid.geometry.cell_center_x(col),
                y: grid.geometry.level_y(zone.level),
                z: grid.geometry.cell_center_z(row),
            };
            let pos = carriers.pose(zone.carrier).transform_position(&local);
            character_spawn_position_is_clear(&pos, collision_world, occupied_positions, character_physics)
                .then_some(pos)
        });
    }
    let valid_cells = collect_valid_cells(map_config.grid(zone.carrier), zone.level, zone.cells());
    pick_clear_position(
        &valid_cells,
        map_config,
        carriers,
        collision_world,
        occupied_positions,
        character_physics,
    )
}

// (carrier, level, col, row) — the cell in its carrier's grid, same axis
// order as the file format's `cols`/`rows` arrays and the editor's drag
// tool. Internally the cell grid is indexed `[row][col]`, but that's local
// to the bounds checks below.
type SpawnCell = (CarrierId, u8, i32, i32);

fn collect_valid_cells(grid: &CarrierGrid, level: u8, cells: impl Iterator<Item = (i32, i32)>) -> Vec<SpawnCell> {
    let Some(level_grid) = grid.levels.get(level as usize) else {
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
            out.push((grid.carrier, level, col, row));
        }
    }
    out
}

// The cell's position is in its carrier's frame; the carrier's pose at this
// tick puts it in the world, where the colliders are. `None` when no cell
// is spawnable or no random spot came up clear.
fn pick_clear_position(
    valid_cells: &[SpawnCell],
    map_config: &MapConfig,
    carriers: &Carriers,
    collision_world: &CollisionWorld,
    occupied_positions: &[Position],
    character_physics: CharacterPhysicsConfig,
) -> Option<Position> {
    sample_clear_position(
        occupied_positions,
        character_physics,
        |_, rng| {
            let &(carrier, level, col, row) = valid_cells.choose(rng)?;
            let geometry = &map_config.grid(carrier).geometry;
            let local = random_position_in_spawn_cell(rng, geometry, level, col, row, character_physics)?;
            Some(carriers.pose(carrier).transform_position(&local))
        },
        |pos| !collision_world.character_overlaps_wall(pos, character_physics),
    )
}

fn random_position_in_spawn_cell(
    rng: &mut ThreadRng,
    geometry: &MapGeometry,
    level: u8,
    col: i32,
    row: i32,
    character_physics: CharacterPhysicsConfig,
) -> Option<Position> {
    if character_physics.movement_collider.diameter > geometry.cell_size() {
        return None;
    }
    let cell_min_x = geometry.cell_to_world_x(col);
    let cell_max_x = cell_min_x + geometry.cell_size();
    let cell_min_z = geometry.cell_to_world_z(row);
    let cell_max_z = cell_min_z + geometry.cell_size();

    Some(Position {
        x: rng.random_range(
            (cell_min_x + character_physics.movement_collider.radius())
                ..=(cell_max_x - character_physics.movement_collider.radius()),
        ),
        y: geometry.level_y(level),
        z: rng.random_range(
            (cell_min_z + character_physics.movement_collider.radius())
                ..=(cell_max_z - character_physics.movement_collider.radius()),
        ),
    })
}

fn character_spawn_position_is_clear(
    pos: &Position,
    collision_world: &CollisionWorld,
    occupied_positions: &[Position],
    character_physics: CharacterPhysicsConfig,
) -> bool {
    !occupied_positions
        .iter()
        .any(|other| character_position_intersects_character(pos, other, character_physics))
        && !collision_world.character_overlaps_wall(pos, character_physics)
}

fn character_position_intersects_character(
    pos: &Position,
    other: &Position,
    character_physics: CharacterPhysicsConfig,
) -> bool {
    character_paths_intersect(pos, pos, character_physics, other, other, character_physics)
}

#[cfg(test)]
#[path = "tests/spawning.rs"]
mod tests;
