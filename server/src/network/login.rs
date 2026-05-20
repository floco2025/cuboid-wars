use bevy::prelude::*;

use crate::{
    characters::generate_player_spawn_position,
    net::ServerToClient,
    resources::{ActorMap, ItemMap, PlayerMap, QuestState},
};
use common::{
    physics::CharacterVerticalVelocity,
    protocol::{ActorMarker, ItemMarker, PlayerMarker, *},
};

use super::{
    broadcast::{collect_items, snapshot_actors, snapshot_logged_in_players},
    incoming::LoginWorld,
};

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
    world: &LoginWorld,
    items: &Res<ItemMap>,
    actors: &Res<ActorMap>,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection, &Health), With<PlayerMarker>>,
    motions: &Query<&CharacterVerticalVelocity, With<PlayerMarker>>,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceDirection, &Health), With<ActorMarker>>,
    actor_motions: &Query<&CharacterVerticalVelocity, With<ActorMarker>>,
    item_positions: &Query<&Position, With<ItemMarker>>,
) {
    match msg {
        ClientMessage::Login(login) => {
            debug!("{:?} logged in", id);

            let channel = {
                let player_info = players
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

                channel
            };

            // Send Init to the connecting player (their ID and map config)
            let init_msg = ServerMessage::Init(SInit {
                id,
                map_layout: (*world.map_layout).clone(),
            });
            if let Err(e) = channel.send(ServerToClient::Send(init_msg)) {
                warn!("failed to send init to {:?}: {}", id, e);
                return;
            }

            // Auto-assign every catalogued quest to the new player. V1 has
            // exactly one quest; this loop is the extension point for a
            // future quest-giver system. Quest state persists for the whole
            // session (cleared neither by death nor by `clear_per_life_state`).
            {
                let player_info = players
                    .get_mut(&id)
                    .expect("handle_login_message called for unknown player");
                for quest in &world.server_gameplay_config.quests {
                    player_info.quest_states.insert(
                        quest.id.clone(),
                        QuestState {
                            progress: 0,
                            completed: false,
                        },
                    );
                    let msg = ServerMessage::QuestNew(SQuestNew {
                        id: quest.id.clone(),
                        announcement_text: quest.announcement_text.clone(),
                    });
                    if let Err(e) = channel.send(ServerToClient::Send(msg)) {
                        warn!("failed to send quest assignment to {:?}: {}", id, e);
                    }
                }
            }

            // Generate random initial position for the new player.
            // Avoid spawning on top of any other logged-in player.
            let occupied_positions: Vec<Position> = players
                .values()
                .filter(|p| p.logged_in && p.entity != entity)
                .filter_map(|p| player_data.get(p.entity).ok())
                .map(|(pos, _, _, _)| *pos)
                .collect();
            let pos = generate_player_spawn_position(
                &world.map_config,
                &world.map_geometry,
                &world.collision_world,
                &occupied_positions,
                world.gameplay_config.player.physics(),
            );

            // Calculate initial facing direction toward center
            let face_dir = (-pos.x).atan2(-pos.z);

            // Initial move-input intent for the new player (idle)
            let move_intent = PlayerMoveIntent::Idle;

            // Construct player data
            let player = players
                .get(&id)
                .expect("handle_login_message called for unknown player")
                .snapshot_player(
                    pos,
                    move_intent,
                    face_dir,
                    Health(world.gameplay_config.player.health().max),
                    0.0,
                );

            // Construct the initial snapshot for the new player
            let mut all_players = snapshot_logged_in_players(players, player_data, motions)
                .into_iter()
                .filter(|(player_id, _)| *player_id != id)
                .collect::<Vec<_>>();
            // Add the new player manually with their freshly generated values
            all_players.push((id, player.clone()));

            // Collect all items for the initial snapshot
            let all_items = collect_items(items, item_positions);
            let all_actors = snapshot_actors(actors, actor_data, actor_motions);

            // Send the initial snapshot to the new player
            let snapshot_msg = ServerMessage::Snapshot(SSnapshot {
                seq: 0,
                players: all_players,
                actors: all_actors,
                items: all_items,
                // Plate state is per-tick; this login-time snapshot defaults
                // to "everything closed". The next broadcast tick will
                // correct it.
                open_barrier_kinds: Vec::new(),
            });
            channel.send(ServerToClient::Send(snapshot_msg)).ok();

            // Now update entity with the authoritative spawn movement state.
            // Other clients learn about this player via the next `SSnapshot`
            // snapshot — no explicit "login" event is broadcast.
            commands.entity(entity).insert((
                pos,
                move_intent,
                FaceDirection(face_dir),
                CharacterVerticalVelocity::default(),
                player.health,
            ));
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
