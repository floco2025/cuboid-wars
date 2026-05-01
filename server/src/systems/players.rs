use bevy::prelude::*;
use rand::{RngExt, rng, rngs::ThreadRng, seq::IndexedRandom};

use super::network::broadcast_to_all;
use crate::resources::{MapConfig, PlayerInfo, PlayerMap};
use common::{
    constants::{
        GRID_CELL_SIZE, LEVEL_HEIGHT, MAP_DEPTH, MAP_WIDTH, PHYSICS_EPSILON, PLAYER_DEATH_Y, PLAYER_DEPTH,
        PLAYER_HEIGHT, PLAYER_WIDTH,
    },
    markers::PlayerMarker,
    physics::{
        CharacterVerticalMotion, CollisionWorld, PlannedCharacterMove, character_paths_intersect,
        overlaps_other_character, step_character_movement,
    },
    protocol::{CharacterMoveIntent, PlayerId, Position, SDeath, ServerMessage},
};

// ============================================================================
// Players Movement System
// ============================================================================

pub fn players_movement_system(
    time: Res<Time>,
    collision_world: Res<CollisionWorld>,
    players: Res<PlayerMap>,
    mut query: Query<
        (
            Entity,
            &mut Position,
            &mut CharacterVerticalMotion,
            &CharacterMoveIntent,
            &PlayerId,
        ),
        With<PlayerMarker>,
    >,
) {
    let delta = time.delta_secs();

    // Pass 1: For each player, calculate intended position and vertical velocity,
    // then apply static-world collision.
    let mut planned_moves: Vec<PlannedCharacterMove> = Vec::new();

    for (entity, pos, motion, move_intent, player_id) in query.iter() {
        // Check if player is stunned
        let is_stunned = players.0.get(player_id).is_some_and(|info| info.stun_timer > 0.0);

        // Compute horizontal velocity from input intent + speed power-up.
        let has_speed_power_up = players.0.get(player_id).is_some_and(PlayerInfo::has_speed);
        let velocity = move_intent.to_player_horizontal_velocity(has_speed_power_up);
        let velocity_sq = velocity.x.mul_add(velocity.x, velocity.z * velocity.z);
        let is_standing_still = velocity_sq < PHYSICS_EPSILON * PHYSICS_EPSILON;
        let suppress_horizontal = is_stunned || is_standing_still;

        let target_xz = if suppress_horizontal {
            *pos
        } else {
            Position {
                x: velocity.x.mul_add(delta, pos.x),
                y: pos.y, // Keep current Y for collision detection
                z: velocity.z.mul_add(delta, pos.z),
            }
        };

        let has_phasing = players.0.get(player_id).is_some_and(PlayerInfo::has_phasing);

        let step = step_character_movement(
            pos,
            motion,
            &collision_world,
            has_phasing,
            target_xz.x,
            target_xz.z,
            delta,
        );

        planned_moves.push(PlannedCharacterMove {
            entity,
            start: *pos,
            target: step.position,
            target_vertical_velocity: step.vertical_velocity,
            blocked: step.blocked,
        });
    }

    // Pass 2: Check player-player collisions and apply final positions
    for planned_move in &planned_moves {
        if let Ok((_, mut pos, mut motion, _, _)) = query.get_mut(planned_move.entity) {
            if overlaps_other_character(planned_move, &planned_moves) {
                pos.y = planned_move.target.y;
            } else {
                *pos = planned_move.target;
            }
            motion.vertical_velocity = planned_move.target_vertical_velocity;
        }
    }
}

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
// Players Death System
// ============================================================================

// Detect players that have fallen below the death threshold and respawn them.
// Broadcasts an `SDeath` message so clients can teleport the entity immediately
// rather than waiting for the next `SUpdate`.
pub fn players_death_system(
    players: Res<PlayerMap>,
    map_config: Res<MapConfig>,
    collision_world: Res<CollisionWorld>,
    mut player_query: Query<(Entity, &PlayerId, &mut Position, &mut CharacterVerticalMotion), With<PlayerMarker>>,
) {
    let dead: Vec<(Entity, PlayerId)> = player_query
        .iter()
        .filter_map(|(entity, id, pos, _)| (pos.y < PLAYER_DEATH_Y).then_some((entity, *id)))
        .collect();

    if dead.is_empty() {
        return;
    }

    // Snapshot all current positions so spawn-distance checks see a consistent view.
    let occupied_positions: Vec<Position> = player_query.iter().map(|(_, _, pos, _)| *pos).collect();

    for (entity, id) in dead {
        let respawn_pos = generate_player_spawn_position(&map_config, &collision_world, &occupied_positions);

        if let Ok((_, _, mut pos, mut motion)) = player_query.get_mut(entity) {
            *pos = respawn_pos;
            motion.vertical_velocity = 0.0;
        }

        info!("{:?} died and respawned at {:?}", id, respawn_pos);
        broadcast_to_all(&players, ServerMessage::Death(SDeath { id, respawn_pos }));
    }
}

// ============================================================================
// Spawn Position Helper
// ============================================================================

const SPAWN_MAX_ATTEMPTS: usize = 100;

// Pick a random clear position on the configured player spawn fields.
// Returns the world origin if no valid placement is found in time.
#[must_use]
pub fn generate_player_spawn_position(
    map_config: &MapConfig,
    collision_world: &CollisionWorld,
    occupied_positions: &[Position],
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
        warn!("no valid player spawn fields (floor without a ramp), spawning at center");
        return Position::default();
    }

    for _ in 0..SPAWN_MAX_ATTEMPTS {
        let &(level, row, col) = valid_cells.choose(&mut rng).expect("valid_cells should not be empty");
        let pos = random_position_in_spawn_cell(&mut rng, level, row, col);

        if player_spawn_position_is_clear(&pos, collision_world, occupied_positions) {
            return pos;
        }
    }

    warn!(
        "could not generate spawn position after {} attempts, spawning at center",
        SPAWN_MAX_ATTEMPTS
    );
    Position::default()
}

fn random_position_in_spawn_cell(rng: &mut ThreadRng, level: u8, row: i32, col: i32) -> Position {
    let cell_min_x = (col as f32).mul_add(GRID_CELL_SIZE, -(MAP_WIDTH / 2.0));
    let cell_max_x = cell_min_x + GRID_CELL_SIZE;
    let cell_min_z = (row as f32).mul_add(GRID_CELL_SIZE, -(MAP_DEPTH / 2.0));
    let cell_max_z = cell_min_z + GRID_CELL_SIZE;

    Position {
        x: rng.random_range((cell_min_x + PLAYER_WIDTH / 2.0)..=(cell_max_x - PLAYER_WIDTH / 2.0)),
        y: f32::from(level) * LEVEL_HEIGHT,
        z: rng.random_range((cell_min_z + PLAYER_DEPTH / 2.0)..=(cell_max_z - PLAYER_DEPTH / 2.0)),
    }
}

fn player_spawn_position_is_clear(
    pos: &Position,
    collision_world: &CollisionWorld,
    occupied_positions: &[Position],
) -> bool {
    let player_center = Vec3::new(pos.x, pos.y + PLAYER_HEIGHT / 2.0, pos.z);
    let player_half_extents = Vec3::new(PLAYER_WIDTH / 2.0, PLAYER_HEIGHT / 2.0, PLAYER_DEPTH / 2.0);

    !occupied_positions
        .iter()
        .any(|other| player_position_intersects_player(pos, other))
        && !collision_world.cuboid_overlaps_wall(player_center, player_half_extents)
}

fn player_position_intersects_player(pos: &Position, other: &Position) -> bool {
    character_paths_intersect(pos, pos, other, other)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::{CellGrid, EdgeGrid, LevelGrid, MapConfig, PlayerSpawnField};
    use common::{
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

        assert!(!player_spawn_position_is_clear(&pos, &collision_world, &[pos]));
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

        assert!(!player_spawn_position_is_clear(
            &Position::default(),
            &collision_world,
            &[]
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

        assert!(player_spawn_position_is_clear(
            &Position::default(),
            &collision_world,
            &[]
        ));
    }

    #[test]
    fn spawn_position_uses_configured_spawn_level() {
        let layout = empty_layout();
        let collision_world = collision_world(&layout);
        let map_config = map_config_with_spawn(1, 0, 0);

        let pos = generate_player_spawn_position(&map_config, &collision_world, &[]);

        assert_eq!(pos.y, LEVEL_HEIGHT);
    }
}
