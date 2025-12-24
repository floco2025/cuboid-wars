use bevy::prelude::*;
use rand::prelude::*;

use crate::{
    net::ServerToClient,
    resources::{GridConfig, ItemMap, PlayerMap, SentryMap},
};
use common::{
    constants::{FIELD_DEPTH, FIELD_WIDTH, GRID_SIZE},
    markers::{ItemMarker, PlayerMarker, SentryMarker},
    protocol::{MapLayout, *},
};

use super::broadcast::{broadcast_to_others, collect_items, collect_sentries, snapshot_logged_in_players};

// ============================================================================
// Login Flow
// ============================================================================

// Handle login message from a player who has not yet logged in.
pub fn handle_login_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: ClientMessage,
    players: &mut ResMut<PlayerMap>,
    map_layout: &Res<MapLayout>,
    grid_config: &Res<GridConfig>,
    items: &Res<ItemMap>,
    sentries: &Res<SentryMap>,
    player_data: &Query<(&Position, &Speed, &FaceDirection), With<PlayerMarker>>,
    item_positions: &Query<&Position, With<ItemMarker>>,
    sentry_data: &Query<(&Position, &Velocity), With<SentryMarker>>,
) {
    match msg {
        ClientMessage::Login(login) => {
            debug!("{:?} logged in", id);

            let (channel, hits, name) = {
                let player_info = players
                    .0
                    .get_mut(&id)
                    .expect("handle_login_message called for unknown player");
                let channel = player_info.channel.clone();
                player_info.logged_in = true;

                // Determine player name: use provided name or default to the player id
                player_info.name = if login.name.is_empty() {
                    format!("Player {}", id.0)
                } else {
                    login.name
                };

                (channel, player_info.hits, player_info.name.clone())
            };

            // Send Init to the connecting player (their ID and grid config)
            let init_msg = ServerMessage::Init(SInit {
                id,
                map_layout: (*map_layout).clone(),
            });
            if let Err(e) = channel.send(ServerToClient::Send(init_msg)) {
                warn!("failed to send init to {:?}: {}", id, e);
                return;
            }

            // Generate random initial position for the new player
            let pos = generate_player_spawn_position(grid_config, players, sentries, player_data, sentry_data);

            // Calculate initial facing direction toward center
            let face_dir = (-pos.x).atan2(-pos.z);

            // Initial speed for the new player
            let speed = Speed {
                speed_level: SpeedLevel::Idle,
                // move_dir: 0.0,
                move_dir: std::f32::consts::PI, // Same as face_dir - facing toward origin
            };

            // Construct player data
            let player = Player::new(name, pos, speed, face_dir, hits);

            // Construct the initial Update for the new player
            let mut all_players = snapshot_logged_in_players(players, player_data)
                .into_iter()
                .filter(|(player_id, _)| *player_id != id)
                .collect::<Vec<_>>();
            // Add the new player manually with their freshly generated values
            all_players.push((id, player.clone()));

            // Collect all items for the initial update
            let all_items = collect_items(items, item_positions);

            // Collect all sentries for the initial update
            let all_sentries = collect_sentries(sentries, sentry_data);

            // Send the initial Update to the new player
            let update_msg = ServerMessage::Update(SUpdate {
                seq: 0,
                players: all_players,
                items: all_items,
                sentries: all_sentries,
            });
            channel.send(ServerToClient::Send(update_msg)).ok();

            // Now update entity: add Position + Speed + FaceDirection
            commands.entity(entity).insert((pos, speed, FaceDirection(face_dir)));

            // Broadcast Login to all other logged-in players
            let login_msg = SLogin { id, player };
            broadcast_to_others(players, id, ServerMessage::Login(login_msg));
        }
        _ => {
            warn!(
                "{:?} sent non-login message before authenticating (likely out-of-order delivery)",
                id
            );
            // Don't despawn - Init message will likely arrive soon
        }
    }
}

// ============================================================================
// Spawn Position Generation
// ============================================================================

// Generate a spawn position in a random grid cell without a ramp,
// spawning in the inner 50% of the cell to avoid walls.
fn generate_player_spawn_position(
    grid_config: &GridConfig,
    players: &PlayerMap,
    sentries: &SentryMap,
    player_data: &Query<(&Position, &Speed, &FaceDirection), With<PlayerMarker>>,
    sentry_data: &Query<(&Position, &Velocity), With<SentryMarker>>,
) -> Position {
    let mut rng = rand::rng();
    let grid_rows = grid_config.grid.len() as i32;
    let grid_cols = grid_config.grid[0].len() as i32;
    let max_attempts = 100;
    const MIN_DISTANCE: f32 = 10.0; // Minimum distance from other entities

    // Collect all cells without ramps
    let mut valid_cells = Vec::new();
    for row in 0..grid_rows {
        for col in 0..grid_cols {
            if !grid_config.grid[row as usize][col as usize].has_ramp {
                valid_cells.push((row, col));
            }
        }
    }

    if valid_cells.is_empty() {
        warn!("no valid spawn cells found (all have ramps), spawning at center");
        return Position::default();
    }

    for _ in 0..max_attempts {
        // Pick a random valid cell
        let &(row, col) = valid_cells.choose(&mut rng).expect("valid_cells should not be empty");

        // Calculate cell center in world coordinates
        let cell_center_x = (col as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_WIDTH / 2.0));
        let cell_center_z = (row as f32 + 0.5).mul_add(GRID_SIZE, -(FIELD_DEPTH / 2.0));

        // Spawn in inner 50% of the cell (25% margin from each edge)
        let spawn_range = GRID_SIZE * 0.5 / 2.0; // 50% of cell size / 2 for radius

        let pos = Position {
            x: cell_center_x + rng.random_range(-spawn_range..=spawn_range),
            y: 0.0,
            z: cell_center_z + rng.random_range(-spawn_range..=spawn_range),
        };

        // Check if position is too close to any existing player
        let too_close_to_player = players
            .0
            .values()
            .filter(|p| p.logged_in)
            .filter_map(|p| player_data.get(p.entity).ok())
            .any(|(p_pos, _, _)| {
                let dx = pos.x - p_pos.x;
                let dz = pos.z - p_pos.z;
                dx.mul_add(dx, dz * dz) < MIN_DISTANCE * MIN_DISTANCE
            });

        // Check if position is too close to any sentry
        let too_close_to_sentry =
            sentries
                .0
                .values()
                .filter_map(|s| sentry_data.get(s.entity).ok())
                .any(|(s_pos, _)| {
                    let dx = pos.x - s_pos.x;
                    let dz = pos.z - s_pos.z;
                    dx.mul_add(dx, dz * dz) < MIN_DISTANCE * MIN_DISTANCE
                });

        if !too_close_to_player && !too_close_to_sentry {
            return pos;
        }
    }

    // Fallback: return center if we somehow failed
    warn!(
        "Could not generate spawn position after {} attempts, spawning at center",
        max_attempts
    );
    Position::default()
}
