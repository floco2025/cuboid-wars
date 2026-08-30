use std::collections::HashMap;

use bevy::prelude::*;

use crate::{
    actors::{ActorMap, PendingActorSpawns},
    characters::{generate_player_spawn_position, spawn_face_yaw},
    items::ItemMap,
    network::{FeedAudience, FeedEvent, ServerToClient, emit_feed},
    players::PlayerMap,
    quests::{QuestBoard, QuestCatalog, assign_quests},
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

// One number per actor kind, sorted by kind for deterministic encoding.
fn sorted_by_kind<V>(per_kind: &HashMap<String, V>, value: impl Fn(&V) -> f32) -> Vec<(String, f32)> {
    let mut values: Vec<(String, f32)> = per_kind.iter().map(|(kind, v)| (kind.clone(), value(v))).collect();
    values.sort_by(|a, b| a.0.cmp(&b.0));
    values
}

// Handle login message from a player who has not yet logged in.
pub fn handle_login_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: ClientMessage,
    players: &mut ResMut<PlayerMap>,
    world: &SharedWorld,
    quest_catalog: &QuestCatalog,
    quest_board: &QuestBoard,
    items: &Res<ItemMap>,
    actors: &Res<ActorMap>,
    pending_spawns: &PendingActorSpawns,
    queries: &CharacterQueries,
    item_positions: &Query<&Position, With<ItemMarker>>,
    rain_intensity: f32,
    lighting: LightingBlend,
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
            let combat = &world.server_gameplay_config.combat;
            let init_msg = ServerMessage::Init(SInit {
                id,
                map_layout: (*world.map_layout).clone(),
                map_settings: (*world.map_settings).clone(),
                actor_blast_radii: sorted_by_kind(&combat.damage.actors, |actor| actor.death_blast.radius),
                player_blast_radius: combat.damage.player_blast.radius,
                missile_blast_radius: combat.damage.missile_blast.radius,
                player_max_health: combat.health.player.max,
                actor_max_health: sorted_by_kind(&combat.health.actors, |actor| actor.max),
            });
            if let Err(e) = channel.send(ServerToClient::Send(init_msg)) {
                warn!("failed to send init to {:?}: {}", id, e);
                return;
            }

            // Assign every unlocked quest to the new player, batched into one
            // `SQuestsAssigned` so the client shows a single combined
            // announcement; quests unlocked later arrive one at a time. Quest
            // state persists for the whole session (cleared neither by death
            // nor by `clear_per_life_state`).
            assign_quests(players, id, quest_catalog, quest_board);

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
                    Health(world.server_gameplay_config.combat.health.player.max),
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
            let (quests, locked_plate_purposes) = quest_board.snapshot_fields(quest_catalog, players);
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
                // correct it.
                open_barrier_kinds: Vec::new(),
                // Real values, like weather and lighting below: a late joiner
                // must see completed quests and active plates immediately.
                quests,
                locked_plate_purposes,
                // Weather and lighting are real so a dark or rainy map
                // doesn't flash bright and dry before the first broadcast.
                rain_intensity,
                lighting,
            });
            channel.send(ServerToClient::Send(snapshot_msg)).ok();

            // Presence is snapshot-only — other clients learn about this
            // player via the next `SSnapshot`; the feed line is cosmetic.
            emit_feed(
                players,
                &world.server_gameplay_config.feed,
                FeedAudience::EveryoneExcept(id),
                FeedEvent::PlayerJoined {
                    name: players.display_name(&id),
                },
            );

            // Now update entity with the authoritative spawn movement state.
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
    use super::{MAX_NAME_CHARS, sanitize_player_name, sorted_by_kind};
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
    fn per_kind_values_are_sorted_and_match_config() {
        let config =
            crate::config::ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        let combat = &config.combat;
        let radii = sorted_by_kind(&combat.damage.actors, |actor| actor.death_blast.radius);
        let max_health = sorted_by_kind(&combat.health.actors, |actor| actor.max);
        for values in [&radii, &max_health] {
            assert_eq!(values.len(), config.actors.len());
            let kinds: Vec<&str> = values.iter().map(|(kind, _)| kind.as_str()).collect();
            let mut sorted = kinds.clone();
            sorted.sort_unstable();
            assert_eq!(kinds, sorted);
        }
        for (kind, radius) in &radii {
            assert_eq!(*radius, combat.damage.expect_actor(kind).death_blast.radius);
        }
        for (kind, max) in &max_health {
            assert_eq!(*max, combat.health.expect_actor(kind).max);
        }
    }
}
