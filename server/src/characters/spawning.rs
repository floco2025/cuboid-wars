use bevy::prelude::*;
use rand::{
    RngExt, rng,
    rngs::ThreadRng,
    seq::{IndexedRandom, SliceRandom},
};

use crate::map::{ActorSpawnZone, CarrierGrid, MapConfig, zone_cells};
use common::{
    config::{ActorGameplayConfig, CharacterPhysicsConfig},
    map::{Carriers, MapGeometry},
    physics::{CollisionWorld, character_paths_intersect},
    protocol::{CarrierId, Checkpoint, FieldId, Position},
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

// Pick a clear position from a single actor spawn zone, on the zone's
// carrier. Used by the actor quota spawner — when topping a specific zone
// up, we never want to spill into other zones. `None` when the zone has no
// clear spot right now; the caller leaves the slot empty rather than spawn
// somewhere the actor does not belong.
#[must_use]
pub fn generate_ground_actor_spawn_position(
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
        return cells.into_iter().find_map(|(level, col, row)| {
            let local = Position {
                x: grid.geometry.cell_center_x(col),
                y: grid.geometry.level_y(level),
                z: grid.geometry.cell_center_z(row),
            };
            let pos = carriers.pose(zone.carrier).transform_position(&local);
            character_spawn_position_is_clear(&pos, collision_world, occupied_positions, character_physics)
                .then_some(pos)
        });
    }
    let valid_cells: Vec<_> = zone
        .level_range()
        .flat_map(|level| collect_valid_cells(map_config.grid(zone.carrier), level, zone.cells()))
        .collect();
    pick_clear_position(
        &valid_cells,
        map_config,
        carriers,
        collision_world,
        occupied_positions,
        character_physics,
    )
}

// A clear spot for the body respawning at the checkpoints numbered
// `number`, on any carrier: their cells are pooled and one is picked
// uniformly at random. Each flag at a rectangle's centre counts as an
// occupied body, so nobody appears inside its pole; the start has none.
// `None` while every spot is blocked.
#[must_use]
pub fn generate_checkpoint_spawn_position(
    map_config: &MapConfig,
    carriers: &Carriers,
    checkpoints: &[Checkpoint],
    number: u32,
    collision_world: &CollisionWorld,
    occupied_positions: &[Position],
    character_physics: CharacterPhysicsConfig,
) -> Option<Position> {
    let mut valid_cells = Vec::new();
    let mut occupied = occupied_positions.to_vec();
    for checkpoint in checkpoints.iter().filter(|checkpoint| checkpoint.number == number) {
        valid_cells.extend(collect_valid_cells(
            map_config.grid(checkpoint.carrier),
            checkpoint.level,
            zone_cells(checkpoint.cols, checkpoint.rows),
        ));
        if number != 0 {
            occupied.push(carriers.pose(checkpoint.carrier).transform_position(&Position {
                x: (checkpoint.min_x + checkpoint.max_x) / 2.0,
                y: checkpoint.y,
                z: (checkpoint.min_z + checkpoint.max_z) / 2.0,
            }));
        }
    }
    pick_clear_position(
        &valid_cells,
        map_config,
        carriers,
        collision_world,
        &occupied,
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
// is spawnable or no random spot came up clear of walls, closed barriers,
// and the other solids a body cannot stand inside.
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
        |pos| !collision_world.character_overlaps_solid(pos, character_physics, &[]),
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

pub(crate) fn generate_flying_spawn_position(
    grid: &CarrierGrid,
    carriers: &Carriers,
    zone: &ActorSpawnZone,
    world: &CollisionWorld,
    occupied: &[(Position, CharacterPhysicsConfig)],
    physics: CharacterPhysicsConfig,
    open: &[FieldId],
) -> Option<Position> {
    let geometry = grid.geometry;
    let radius = physics.movement_collider.radius();
    let min = Vec3::new(
        geometry.cell_to_world_x(zone.cols[0]) + radius,
        geometry.level_y(zone.level) + 0.04,
        geometry.cell_to_world_z(zone.rows[0]) + radius,
    );
    let max = Vec3::new(
        geometry.cell_to_world_x(zone.cols[1]) - radius,
        geometry.level_y(zone.level) + f32::from(zone.levels) * geometry.level_height()
            - physics.movement_collider.height
            - 0.04,
        geometry.cell_to_world_z(zone.rows[1]) - radius,
    );
    if min.cmpgt(max).any() {
        return None;
    }
    let mut rng = rng();
    (0..SPAWN_MAX_ATTEMPTS).find_map(|_| {
        let local = Vec3::new(
            rng.random_range(min.x..=max.x),
            rng.random_range(min.y..=max.y),
            rng.random_range(min.z..=max.z),
        );
        let pos = Position::from(carriers.pose(zone.carrier).transform_point(local));
        (!world.character_overlaps_solid(&pos, physics, open)
            && !occupied
                .iter()
                .any(|(other, body)| character_paths_intersect(&pos, &pos, physics, other, other, *body)))
        .then_some(pos)
    })
}
