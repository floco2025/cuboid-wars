use bevy::prelude::*;

use crate::{
    network::{FeedAudience, FeedEvent, ServerToClient, emit_feed},
    players::{PlayerMap, enter_group_respawn, place_player_body, player_spawn_destination, spawn_zone_destination},
    portals::{PortalAssignments, PortalMap},
    quests::{QuestBoard, QuestCatalog, assign_quests},
};
use common::protocol::*;

use super::handlers::{CharacterQueries, SharedWorld};

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

// `SInit` goes out first on the reliable lane; everything after it in
// this function follows in order; blocked spawns wait without a body.
pub(super) fn handle_login_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    message: CLogin,
    players: &mut PlayerMap,
    world: &SharedWorld,
    queries: &CharacterQueries,
    quest_catalog: &QuestCatalog,
    quest_board: &QuestBoard,
    portal_assignments: &mut PortalAssignments,
    portals: &mut PortalMap,
) {
    let shared_checkpoint = players.shared_checkpoint;
    let Some(player_info) = players.get_mut(&id) else {
        error!("registered player#{} missing during login", id.0);
        return;
    };
    player_info.connection.logged_in = true;
    player_info.session.checkpoint = shared_checkpoint;
    player_info.connection.name = sanitize_player_name(&message.name, id);
    let channel = player_info.connection.channel.clone();
    debug!("{} authenticated", players.describe(&id));

    let portal_access = portal_assignments.assign(id);
    // A fresh assignment starts with no placed ends. Only a lone `single`
    // player's second end can be here: this player now controls it.
    portals.remove_access(portal_access);
    let init_message = ServerMessage::Init(SInit {
        player: PlayerBootstrap { id, portal_access },
        world: (*world.world_bootstrap).clone(),
    });
    if let Err(error) = channel.send(ServerToClient::Send(init_message)) {
        warn!("failed to send init to {:?}: {}", id, error);
    }

    assign_quests(players, id, quest_catalog, quest_board);

    // Presence remains snapshot-owned; this line is cosmetic.
    emit_feed(
        players,
        &world.server_gameplay_config.feed,
        FeedAudience::EveryoneExcept(id),
        FeedEvent::PlayerJoined {
            name: players.display_name(&id),
        },
    );

    let occupied_positions: Vec<Position> = players
        .values()
        .filter(|player| player.connection.logged_in && player.entity() != Some(entity))
        .filter_map(|player| player.entity().and_then(|entity| queries.player_data.get(entity).ok()))
        .map(|(pos, _, _)| *pos)
        .collect();
    let physics = world.gameplay_config.player.physics();
    let spawn = player_spawn_destination(
        &world.map_config,
        &world.map_layout.checkpoints,
        &world.carriers,
        &world.collision_world,
        &occupied_positions,
        physics,
        shared_checkpoint,
    )
    .unwrap_or_else(|| {
        // A joining player gets a body now; the blocked checkpoint stays saved for the next respawn.
        info!(
            "{}: the shared checkpoint is blocked, spawning in a zone instead",
            players.describe(&id)
        );
        spawn_zone_destination(
            &world.map_config,
            &world.map_layout.checkpoints,
            &world.carriers,
            &world.collision_world,
            &occupied_positions,
            physics,
        )
    });
    if enter_group_respawn(commands, players, id, spawn.pos) {
        return;
    }
    place_player_body(
        commands,
        players,
        id,
        entity,
        &spawn,
        Health(world.server_gameplay_config.combat.health.player.max),
        world.tick.0,
        portal_access,
    );
}

#[cfg(test)]
#[path = "tests/login.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/login_checkpoint.rs"]
mod checkpoint_tests;
