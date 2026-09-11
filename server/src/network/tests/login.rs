use super::{MAX_NAME_CHARS, sanitize_player_name};
use crate::config::ServerGameplayConfig;
use common::protocol::{
    BarrierKindId, HexColor, ItemType, KindDef, MapBootstrap, MapItems, MapLayout, MapSettings, PlateState,
    PlayerBootstrap, PlayerId, PortalAccess, SInit, ServerMessage, WorldBootstrap,
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
    assert_eq!(sanitize_player_name("Alex", PlayerId(1)), "Alex");
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
        plates: PlateState::default(),
        locked_switches: Vec::new(),
        world: WorldBootstrap {
            network: Default::default(),
            gameplay: config.gameplay_bootstrap(),
            map: MapBootstrap {
                missile_air_grids: Vec::new(),
                layout: MapLayout::default(),
                settings: MapSettings {
                    barrier_kinds: vec![
                        KindDef {
                            id: "lobby".to_owned(),
                            color: HexColor([0x22, 0xcc, 0x33]),
                        },
                        KindDef {
                            id: "basement".to_owned(),
                            color: HexColor([0xf0, 0xc0, 0x20]),
                        },
                    ],
                    ..map_settings
                },
                items: MapItems(vec![ItemType::Key(BarrierKindId(1))]),
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
    let kinds = &decoded.world.map.settings.barrier_kinds;
    assert_eq!(
        kinds.iter().map(|kind| kind.id.as_str()).collect::<Vec<_>>(),
        ["lobby", "basement"]
    );
    assert_eq!(kinds[1].color, HexColor([0xf0, 0xc0, 0x20]));
    assert_eq!(decoded.world.map.items.key_kinds(), [BarrierKindId(1)]);
    assert_eq!(decoded.world.gameplay.actors.len(), config.actors.kinds.len());
}
