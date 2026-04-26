use bevy::prelude::*;

use crate::{
    net::ServerToClient,
    resources::{GridConfig, ItemMap, PlayerMap},
    systems::generate_player_spawn_position,
};
use common::{
    markers::{ItemMarker, PlayerMarker},
    physics::PlayerMotion,
    protocol::{MapLayout, *},
};

use super::broadcast::{broadcast_to_others, collect_items, snapshot_logged_in_players};

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
    player_data: &Query<(&Position, &MoveInput, &FaceDirection), With<PlayerMarker>>,
    motions: &Query<&PlayerMotion, With<PlayerMarker>>,
    item_positions: &Query<&Position, With<ItemMarker>>,
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

            // Generate random initial position for the new player.
            // Avoid spawning on top of any other logged-in player.
            let occupied_positions: Vec<Position> = players
                .0
                .values()
                .filter(|p| p.logged_in && p.entity != entity)
                .filter_map(|p| player_data.get(p.entity).ok())
                .map(|(pos, _, _)| *pos)
                .collect();
            let pos = generate_player_spawn_position(grid_config, &occupied_positions);

            // Calculate initial facing direction toward center
            let face_dir = (-pos.x).atan2(-pos.z);

            // Initial move-input intent for the new player (idle)
            let move_input = MoveInput::Idle;

            // Construct player data
            let player = Player::new(name, pos, move_input, face_dir, hits);

            // Construct the initial Update for the new player
            let mut all_players = snapshot_logged_in_players(players, player_data, motions)
                .into_iter()
                .filter(|(player_id, _)| *player_id != id)
                .collect::<Vec<_>>();
            // Add the new player manually with their freshly generated values
            all_players.push((id, player.clone()));

            // Collect all items for the initial update
            let all_items = collect_items(items, item_positions);

            // Send the initial Update to the new player
            let update_msg = ServerMessage::Update(SUpdate {
                seq: 0,
                players: all_players,
                items: all_items,
            });
            channel.send(ServerToClient::Send(update_msg)).ok();

            // Now update entity: add Position + MoveInput + FaceDirection + PlayerMotion
            commands
                .entity(entity)
                .insert((pos, move_input, FaceDirection(face_dir), PlayerMotion::default()));

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
