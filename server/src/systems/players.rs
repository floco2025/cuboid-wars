use bevy::prelude::*;
use rand::{RngExt, rng, rngs::ThreadRng, seq::IndexedRandom};

use super::network::broadcast_to_all;
use crate::resources::{MapConfig, PlayerMap};
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    constants::{CHARACTER_FALL_TELEPORT_Y, GRID_CELL_SIZE, LEVEL_HEIGHT, MAP_DEPTH, MAP_WIDTH},
    markers::PlayerMarker,
    physics::{CharacterVerticalMotion, CollisionWorld, character_paths_intersect},
    protocol::{CharacterMoveIntent, CharacterMovementState, PlayerId, Position, SPlayerTeleport, ServerMessage},
};

// ============================================================================
// Players Timer System
// ============================================================================

// System to count down player power-up and stun timers
pub fn players_timer_system(time: Res<Time>, mut players: ResMut<PlayerMap>) {
    let delta = time.delta_secs();

    let mut status_messages = Vec::new();

    for (player_id, player_info) in &mut players.0 {
        let old_status = player_info.status(*player_id);

        player_info.tick_timers(delta);

        let new_status = player_info.status(*player_id);

        if old_status != new_status {
            status_messages.push(new_status);
        }
    }

    // Send status updates to all clients
    for msg in status_messages {
        broadcast_to_all(&players, ServerMessage::PlayerStatus(msg));
    }
}

// ============================================================================
// Players Fall Recovery System
// ============================================================================

// Detect players that have fallen below the death threshold and move them back
// to a spawn position. Broadcasts a teleport so clients apply it immediately
// rather than waiting for the next `SUpdate`.
pub fn players_fall_recovery_system(
    players: Res<PlayerMap>,
    map_config: Res<MapConfig>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    mut player_query: Query<
        (
            Entity,
            &PlayerId,
            &mut Position,
            &mut CharacterVerticalMotion,
            &CharacterMoveIntent,
        ),
        With<PlayerMarker>,
    >,
) {
    let dead: Vec<(Entity, PlayerId)> = player_query
        .iter()
        .filter_map(|(entity, id, pos, _, _)| (pos.y < CHARACTER_FALL_TELEPORT_Y).then_some((entity, *id)))
        .collect();

    if dead.is_empty() {
        return;
    }

    // Snapshot all current positions so spawn-distance checks see a consistent view.
    let occupied_positions: Vec<Position> = player_query.iter().map(|(_, _, pos, _, _)| *pos).collect();

    for (entity, id) in dead {
        let teleport_pos = generate_character_spawn_position(
            &map_config,
            &collision_world,
            &occupied_positions,
            gameplay_config.characters.player.physics(),
        );

        if let Ok((_, _, mut pos, mut motion, move_intent)) = player_query.get_mut(entity) {
            *pos = teleport_pos;
            motion.0 = 0.0;
            broadcast_to_all(
                &players,
                ServerMessage::PlayerTeleport(SPlayerTeleport {
                    id,
                    movement: CharacterMovementState::new(teleport_pos, *move_intent, 0.0),
                }),
            );
        }

        info!("{:?} fell and teleported to {:?}", id, teleport_pos);
    }
}

// ============================================================================
// Spawn Position Helper
// ============================================================================

const SPAWN_MAX_ATTEMPTS: usize = 100;

// Pick a random clear position on the configured spawn fields.
// Returns the world origin if no valid placement is found in time.
#[must_use]
pub fn generate_character_spawn_position(
    map_config: &MapConfig,
    collision_world: &CollisionWorld,
    occupied_positions: &[Position],
    character_physics: CharacterPhysicsConfig,
) -> Position {
    let mut rng = rng();

    let mut valid_cells = Vec::new();
    for field in &map_config.player_spawn_fields {
        let Some(level_grid) = map_config.levels.get(field.level as usize) else {
            continue;
        };
        let cells = &level_grid.cells.rows;
        if field.row >= 0
            && field.row < cells.len() as i32
            && field.col >= 0
            && field.col < cells[field.row as usize].len() as i32
        {
            let cell = &cells[field.row as usize][field.col as usize];
            if cell.has_floor && !cell.has_ramp {
                valid_cells.push((field.level, field.row, field.col));
            }
        }
    }

    if valid_cells.is_empty() {
        warn!("no valid spawn fields (floor without a ramp), spawning at center");
        return Position::default();
    }

    for _ in 0..SPAWN_MAX_ATTEMPTS {
        let &(level, row, col) = valid_cells.choose(&mut rng).expect("valid_cells should not be empty");
        let pos = random_position_in_spawn_cell(&mut rng, level, row, col, character_physics);

        if character_spawn_position_is_clear(&pos, collision_world, occupied_positions, character_physics) {
            return pos;
        }
    }

    warn!(
        "could not generate spawn position after {} attempts, spawning at center",
        SPAWN_MAX_ATTEMPTS
    );
    Position::default()
}

fn random_position_in_spawn_cell(
    rng: &mut ThreadRng,
    level: u8,
    row: i32,
    col: i32,
    character_physics: CharacterPhysicsConfig,
) -> Position {
    let cell_min_x = (col as f32).mul_add(GRID_CELL_SIZE, -(MAP_WIDTH / 2.0));
    let cell_max_x = cell_min_x + GRID_CELL_SIZE;
    let cell_min_z = (row as f32).mul_add(GRID_CELL_SIZE, -(MAP_DEPTH / 2.0));
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
    use crate::resources::{CellGrid, EdgeGrid, LevelGrid, MapConfig, PlayerSpawnField};
    use common::{
        config::GameplayConfig,
        constants::WALL_THICKNESS,
        protocol::{MapLayout, Wall},
    };

    fn empty_layout() -> MapLayout {
        MapLayout {
            walls: Vec::new(),
            ramps: Vec::new(),
            floors: Vec::new(),
            wall_lights: Vec::new(),
        }
    }

    fn collision_world(layout: &MapLayout) -> CollisionWorld {
        CollisionWorld::from_map_layout(layout)
    }

    fn character_physics() -> CharacterPhysicsConfig {
        GameplayConfig::load_default()
            .expect("default gameplay config should load")
            .characters
            .player
            .physics()
    }

    fn map_config_with_spawn(level: u8, col: i32, row: i32) -> MapConfig {
        let mut levels = (0..=level)
            .map(|_| LevelGrid {
                cells: CellGrid::new(2, 2),
                edges: EdgeGrid::new(2, 2),
            })
            .collect::<Vec<_>>();
        levels[usize::from(level)].cells.rows[row as usize][col as usize].has_floor = true;
        MapConfig {
            levels,
            player_spawn_fields: vec![PlayerSpawnField { level, col, row }],
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
    fn spawn_position_uses_configured_spawn_level() {
        let layout = empty_layout();
        let collision_world = collision_world(&layout);
        let map_config = map_config_with_spawn(1, 0, 0);

        let pos = generate_character_spawn_position(&map_config, &collision_world, &[], character_physics());

        assert_eq!(pos.y, LEVEL_HEIGHT);
    }
}
