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
    handlers::{CharacterQueries, SharedWorld},
};

const MAX_NAME_CHARS: usize = 32;

// Names ride every snapshot, so keep malformed input bounded and displayable.
fn sanitize_player_name(raw: &str, id: PlayerId) -> String {
    let sanitized: String = raw.chars().filter(|c| !c.is_control()).take(MAX_NAME_CHARS).collect();
    if sanitized.trim().is_empty() {
        format!("Player {}", id.0)
    } else {
        sanitized
    }
}

fn sorted_by_kind<V>(per_kind: &HashMap<String, V>, value: impl Fn(&V) -> f32) -> Vec<(String, f32)> {
    let mut values: Vec<(String, f32)> = per_kind.iter().map(|(kind, v)| (kind.clone(), value(v))).collect();
    values.sort_by(|a, b| a.0.cmp(&b.0));
    values
}

#[expect(
    clippy::too_many_arguments,
    reason = "login assembles the initial state from each server domain"
)]
pub(super) fn handle_login_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    message: CLogin,
    players: &mut PlayerMap,
    world: &SharedWorld,
    quest_catalog: &QuestCatalog,
    quest_board: &QuestBoard,
    items: &ItemMap,
    actors: &ActorMap,
    pending_spawns: &PendingActorSpawns,
    queries: &CharacterQueries,
    item_positions: &Query<&Position, With<ItemMarker>>,
    rain_intensity: f32,
    lighting: LightingBlend,
) {
    let Some(player_info) = players.get_mut(&id) else {
        error!("registered player#{} missing during login", id.0);
        return;
    };
    player_info.connection.logged_in = true;
    player_info.connection.name = sanitize_player_name(&message.name, id);
    let channel = player_info.connection.channel.clone();
    debug!("{} logged in", players.describe(&id));

    let combat = &world.server_gameplay_config.combat;
    let init_message = ServerMessage::Init(SInit {
        id,
        map_layout: (*world.map_layout).clone(),
        map_settings: (*world.map_settings).clone(),
        actor_blast_radii: sorted_by_kind(&combat.damage.actors, |actor| actor.death_blast.radius),
        player_blast_radius: combat.damage.player_blast.radius,
        missile_blast_radius: combat.damage.missile_blast.radius,
        player_max_health: combat.health.player.max,
        actor_max_health: sorted_by_kind(&combat.health.actors, |actor| actor.max),
    });
    if let Err(error) = channel.send(ServerToClient::Send(init_message)) {
        warn!("failed to send init to {:?}: {}", id, error);
        return;
    }

    // Batch initially unlocked quests so login produces one announcement.
    assign_quests(players, id, quest_catalog, quest_board);

    let occupied_positions: Vec<Position> = players
        .values()
        .filter(|player| player.connection.logged_in && player.entity() != Some(entity))
        .filter_map(|player| player.entity().and_then(|entity| queries.player_data.get(entity).ok()))
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
    let move_intent = PlayerMoveIntent::Idle;

    let Some(player_info) = players.get(&id) else {
        error!("registered player#{} missing while building login snapshot", id.0);
        return;
    };
    let player = player_info.snapshot_player(
        pos,
        move_intent,
        face_yaw,
        Health(world.server_gameplay_config.combat.health.player.max),
        0.0,
    );

    let mut all_players = snapshot_logged_in_players(players, &queries.player_data, &queries.player_motions)
        .into_iter()
        .filter(|(player_id, _)| *player_id != id)
        .collect::<Vec<_>>();
    all_players.push((id, player.clone()));

    let all_items = collect_items(items, item_positions);
    let all_actors = snapshot_actors(actors, &queries.actor_data, &queries.actor_motions);
    let (quests, locked_plate_purposes) = quest_board.snapshot_fields(quest_catalog, players);
    let snapshot_message = ServerMessage::Snapshot(SSnapshot {
        seq: 0,
        players: all_players,
        actors: all_actors,
        // Late joiners must see beam-in ghosts that are already in progress.
        spawning_actors: snapshot_spawning_actors(pending_spawns),
        items: all_items,
        // In-flight missiles arrive with the next regular snapshot.
        missiles: Vec::new(),
        // Per-tick plate state is corrected by the next regular snapshot.
        open_barrier_kinds: Vec::new(),
        // Durable quest state must be correct immediately for late joiners.
        quests,
        locked_plate_purposes,
        // Live conditions prevent a bright, dry flash while joining.
        rain_intensity,
        lighting,
    });
    channel.send(ServerToClient::Send(snapshot_message)).ok();

    // Presence remains snapshot-owned; this line is cosmetic.
    emit_feed(
        players,
        &world.server_gameplay_config.feed,
        FeedAudience::EveryoneExcept(id),
        FeedEvent::PlayerJoined {
            name: players.display_name(&id),
        },
    );

    commands.entity(entity).insert((
        pos,
        move_intent,
        FaceYaw(face_yaw),
        CharacterVerticalVelocity::default(),
        player.health,
    ));
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
            crate::config::ServerGameplayConfig::load_default().expect("default server gameplay config failed to load");
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
