use bevy::prelude::*;

use crate::{
    characters::{generate_player_spawn_position, spawn_face_yaw},
    network::{FeedAudience, FeedEvent, ServerToClient, emit_feed},
    players::{PlayerMap, UnlimitedMissiles},
    portals::{PortalAssignments, PortalMap},
    quests::{QuestBoard, QuestCatalog, assign_quests},
};
use common::{
    physics::{CharacterVerticalVelocity, PortalSet},
    protocol::*,
};

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
// this function follows in order, and the body exists from here on.
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
    portal_set: &mut PortalSet,
    unlimited_missiles: &UnlimitedMissiles,
) {
    let Some(player_info) = players.get_mut(&id) else {
        error!("registered player#{} missing during login", id.0);
        return;
    };
    player_info.connection.logged_in = true;
    player_info.connection.name = sanitize_player_name(&message.name, id);
    if unlimited_missiles.0 {
        player_info.life.missiles = world.gameplay_config.missiles.max_missiles;
    }
    let channel = player_info.connection.channel.clone();
    debug!("{} authenticated", players.describe(&id));

    let portal_access = portal_assignments.assign(id);
    // A fresh assignment starts with no placed ends. Only a lone `single`
    // player's second end can be here: this player now controls it.
    if portals.remove_access(portal_access) {
        *portal_set = portals.rebuild_set(&world.collision_world);
    }
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
        .map(|(pos, _, _, _)| *pos)
        .collect();
    let pos = generate_player_spawn_position(
        &world.map_config,
        &world.map_geometry,
        &world.collision_world,
        &occupied_positions,
        world.gameplay_config.player.physics(),
    );
    commands.entity(entity).insert((
        pos,
        PlayerMoveIntent::Idle,
        FaceYaw(spawn_face_yaw(&pos)),
        CharacterVerticalVelocity::default(),
        Health(world.server_gameplay_config.combat.health.player.max),
    ));
}

#[cfg(test)]
mod tests {
    use super::{MAX_NAME_CHARS, sanitize_player_name};
    use crate::config::ServerGameplayConfig;
    use common::protocol::{
        BarrierKindId, HexColor, KindDef, MapBootstrap, MapLayout, MapSettings, PlayerBootstrap, PlayerId,
        PortalAccess, SInit, ServerMessage, WorldBootstrap,
    };

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
    fn bootstrap_actor_values_are_sorted_and_match_config() {
        let config = ServerGameplayConfig::load_default().expect("default server gameplay config failed to load");
        let actors = config.gameplay_bootstrap().actors;
        let combat = &config.combat;
        assert_eq!(actors.len(), config.actors.kinds.len());
        let kinds: Vec<&str> = actors.iter().map(|(kind, _)| kind.as_str()).collect();
        let mut sorted = kinds.clone();
        sorted.sort_unstable();
        assert_eq!(kinds, sorted);
        for (kind, actor) in &actors {
            assert_eq!(
                actor.death_blast_radius,
                combat.damage.expect_actor(kind).death_blast.radius
            );
            assert_eq!(actor.max_health, combat.health.expect_actor(kind).max);
        }
    }

    #[test]
    fn init_message_round_trips_complete_bootstrap() {
        let config = ServerGameplayConfig::load_default().expect("default server gameplay config failed to load");
        let map_settings = config
            .maps
            .get(&config.default_map)
            .expect("default map settings missing")
            .settings
            .clone();
        let message = ServerMessage::Init(SInit {
            player: PlayerBootstrap {
                id: PlayerId(7),
                portal_access: PortalAccess::None,
            },
            world: WorldBootstrap {
                gameplay: config.gameplay_bootstrap(),
                map: MapBootstrap {
                    layout: MapLayout::default(),
                    settings: MapSettings {
                        barrier_kinds: Some(vec![
                            KindDef {
                                id: "lobby".to_owned(),
                                color: HexColor([0x22, 0xcc, 0x33]),
                            },
                            KindDef {
                                id: "basement".to_owned(),
                                color: HexColor([0xf0, 0xc0, 0x20]),
                            },
                        ]),
                        ..map_settings
                    },
                    key_kinds: vec![BarrierKindId(1)],
                },
            },
        });

        let bytes = bincode::encode_to_vec(&message, bincode::config::standard()).expect("encode SInit");
        let (decoded, _): (ServerMessage, _) =
            bincode::decode_from_slice(&bytes, bincode::config::standard()).expect("decode SInit");
        let ServerMessage::Init(decoded) = decoded else {
            panic!("decoded message was not SInit");
        };
        assert_eq!(decoded.player.id, PlayerId(7));
        let kinds = decoded.world.map.settings.barrier_kind_defs();
        assert_eq!(
            kinds.iter().map(|kind| kind.id.as_str()).collect::<Vec<_>>(),
            ["lobby", "basement"]
        );
        assert_eq!(kinds[1].color, HexColor([0xf0, 0xc0, 0x20]));
        assert_eq!(decoded.world.map.key_kinds, [BarrierKindId(1)]);
        assert_eq!(decoded.world.gameplay.actors.len(), config.actors.kinds.len());
    }
}
