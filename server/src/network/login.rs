use bevy::prelude::*;

use crate::{
    actors::{ActorMap, PendingActorSpawns},
    characters::{generate_player_spawn_position, spawn_face_yaw},
    items::ItemMap,
    network::ServerToClient,
    players::{PlayerMap, assign_quests},
};
use common::{
    physics::CharacterVerticalVelocity,
    protocol::{ItemMarker, *},
};

use super::{
    broadcast::{collect_items, snapshot_actors, snapshot_logged_in_players, snapshot_spawning_actors},
    incoming::{CharacterQueries, SharedWorld},
};

// ============================================================================
// Login Flow
// ============================================================================

// Player names are echoed in every snapshot, so an unbounded/control-laden
// name from a malformed client is a broadcast-amplified cost. Cap the
// displayable length and strip control characters; fall back to a default when
// nothing usable remains.
const MAX_NAME_CHARS: usize = 32;

fn sanitize_player_name(raw: &str, id: PlayerId) -> String {
    let sanitized: String = raw.chars().filter(|c| !c.is_control()).take(MAX_NAME_CHARS).collect();
    if sanitized.trim().is_empty() {
        format!("Player {}", id.0)
    } else {
        sanitized
    }
}

fn actor_explosion_radii(config: &crate::config::ServerGameplayConfig) -> Vec<(String, f32)> {
    let mut radii: Vec<(String, f32)> = config
        .actors
        .iter()
        .map(|(kind, actor)| (kind.clone(), actor.combat.death_explosion.radius))
        .collect();
    radii.sort_by(|a, b| a.0.cmp(&b.0));
    radii
}

// Handle login message from a player who has not yet logged in.
pub fn handle_login_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: ClientMessage,
    players: &mut ResMut<PlayerMap>,
    world: &SharedWorld,
    items: &Res<ItemMap>,
    actors: &Res<ActorMap>,
    pending_spawns: &PendingActorSpawns,
    queries: &CharacterQueries,
    item_positions: &Query<&Position, With<ItemMarker>>,
) {
    match msg {
        ClientMessage::Login(login) => {
            let channel = {
                let player_info = players
                    .get_mut(&id)
                    .expect("handle_login_message called for unknown player");
                let channel = player_info.channel.clone();
                player_info.logged_in = true;

                player_info.name = sanitize_player_name(&login.name, id);

                channel
            };
            debug!("{} logged in", players.describe(&id));

            // Send Init to the connecting player (their ID and map config)
            let init_msg = ServerMessage::Init(SInit {
                id,
                map_layout: (*world.map_layout).clone(),
                map_settings: (*world.map_settings).clone(),
                actor_explosion_radii: actor_explosion_radii(&world.server_gameplay_config),
                player_explosion_radius: world.server_gameplay_config.player.explosion.radius,
            });
            if let Err(e) = channel.send(ServerToClient::Send(init_msg)) {
                warn!("failed to send init to {:?}: {}", id, e);
                return;
            }

            // Auto-assign every catalogued quest to the new player, batched
            // into one `SQuestsAssigned` so the client shows a single combined
            // announcement. `assign_quests` is the shared seam a future in-game
            // quest-giver calls too. Quest state persists for the whole session
            // (cleared neither by death nor by `clear_per_life_state`).
            {
                let player_info = players
                    .get_mut(&id)
                    .expect("handle_login_message called for unknown player");
                if let Some(assigned) = assign_quests(player_info, &world.server_gameplay_config.quests) {
                    let msg = ServerMessage::QuestsAssigned(assigned);
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
                .filter_map(|p| queries.player_data.get(p.entity).ok())
                .map(|(pos, _, _, _)| *pos)
                .collect();
            let pos = generate_player_spawn_position(
                &world.map_config,
                &world.map_geometry,
                &world.collision_world,
                &occupied_positions,
                world.gameplay_config.player.physics(),
            );

            let face_yaw = spawn_face_yaw(&pos);

            // Initial move-input intent for the new player (idle)
            let move_intent = PlayerMoveIntent::Idle;

            // Construct player data
            let player = players
                .get(&id)
                .expect("handle_login_message called for unknown player")
                .snapshot_player(
                    pos,
                    move_intent,
                    face_yaw,
                    Health(world.gameplay_config.player.health().max),
                    0.0,
                );

            // Construct the initial snapshot for the new player
            let mut all_players = snapshot_logged_in_players(players, &queries.player_data, &queries.player_motions)
                .into_iter()
                .filter(|(player_id, _)| *player_id != id)
                .collect::<Vec<_>>();
            // Add the new player manually with their freshly generated values
            all_players.push((id, player.clone()));

            // Collect all items for the initial snapshot
            let all_items = collect_items(items, item_positions);
            let all_actors = snapshot_actors(actors, &queries.actor_data, &queries.actor_motions);

            // Send the initial snapshot to the new player
            let snapshot_msg = ServerMessage::Snapshot(SSnapshot {
                seq: 0,
                players: all_players,
                actors: all_actors,
                // Late joiners see beam-in ghosts already in progress.
                spawning_actors: snapshot_spawning_actors(pending_spawns),
                items: all_items,
                // Any in-flight missiles arrive with the next broadcast tick.
                missiles: Vec::new(),
                // Plate state is per-tick; this login-time snapshot defaults
                // to "everything closed". The next broadcast tick will
                // correct it. Same for weather and lighting — the next
                // broadcast delivers the real values within 250 ms and the
                // client smooths in.
                open_barrier_kinds: Vec::new(),
                rain_intensity: 0.0,
                lighting: Lighting::default(),
            });
            channel.send(ServerToClient::Send(snapshot_msg)).ok();

            // Now update entity with the authoritative spawn movement state.
            // Other clients learn about this player via the next `SSnapshot`
            // snapshot — no explicit "login" event is broadcast.
            commands.entity(entity).insert((
                pos,
                move_intent,
                FaceYaw(face_yaw),
                CharacterVerticalVelocity::default(),
                player.health,
            ));
        }
        _ => {
            warn!(
                "{:?} sent non-login message before authenticating (likely out-of-order delivery)",
                id
            );
            // Don't despawn - the Login message will likely arrive soon
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_NAME_CHARS, actor_explosion_radii, sanitize_player_name};
    use common::protocol::PlayerId;

    #[test]
    fn empty_name_falls_back_to_default() {
        assert_eq!(sanitize_player_name("", PlayerId(7)), "Player 7");
    }

    #[test]
    fn whitespace_only_name_falls_back_to_default() {
        assert_eq!(sanitize_player_name("   \t  ", PlayerId(3)), "Player 3");
    }

    #[test]
    fn control_characters_are_stripped() {
        assert_eq!(sanitize_player_name("a\nb\u{7}c", PlayerId(1)), "abc");
    }

    #[test]
    fn over_long_name_is_truncated_to_cap() {
        let long = "x".repeat(MAX_NAME_CHARS + 50);
        assert_eq!(sanitize_player_name(&long, PlayerId(1)).chars().count(), MAX_NAME_CHARS);
    }

    #[test]
    fn ordinary_name_is_preserved() {
        assert_eq!(sanitize_player_name("Marc", PlayerId(1)), "Marc");
    }

    #[test]
    fn actor_explosion_radii_are_sorted_and_match_config() {
        let config =
            crate::config::ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        let radii = actor_explosion_radii(&config);
        assert_eq!(radii.len(), config.actors.len());
        let kinds: Vec<&str> = radii.iter().map(|(kind, _)| kind.as_str()).collect();
        let mut sorted = kinds.clone();
        sorted.sort_unstable();
        assert_eq!(kinds, sorted);
        for (kind, radius) in &radii {
            assert_eq!(*radius, config.expect_actor(kind).combat.death_explosion.radius);
        }
    }
}
