use bevy::prelude::*;

use crate::{
    characters::MovementStart,
    network::{FeedAudience, FeedEvent, ServerToClient, emit_feed},
    players::{PlayerMap, enter_group_respawn, player_spawn_destination},
    portals::{PortalAssignments, PortalMap},
    quests::{QuestBoard, QuestCatalog, assign_quests},
};
use common::{
    physics::{AirborneMomentum, CharacterVerticalVelocity, KnockbackVelocity, PortalSet},
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
    portal_set: &mut PortalSet,
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
    if portals.remove_access(portal_access) {
        *portal_set = portals.rebuild_set(&world.collision_world, &world.carriers);
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
    let spawn = player_spawn_destination(
        &world.map_config,
        &world.carriers,
        &world.collision_world,
        &occupied_positions,
        world.gameplay_config.player.physics(),
        shared_checkpoint,
    );
    if enter_group_respawn(
        commands,
        players,
        id,
        spawn.as_ref().map_or_else(Position::default, |spawn| spawn.pos),
    ) {
        return;
    }
    let info = players.get_mut(&id).expect("logged-in player missing");
    let Some(spawn) = spawn else {
        info.wait_for_spawn();
        commands.entity(entity).despawn();
        return;
    };
    info.life.checkpoint_contact = spawn.contact;
    commands.entity(entity).insert((
        spawn.pos,
        MovementStart(spawn.pos),
        PlayerMoveIntent::Idle,
        FaceYaw(spawn.face_yaw),
        CharacterVerticalVelocity::default(),
        AirborneMomentum::default(),
        KnockbackVelocity::default(),
        Health(world.server_gameplay_config.combat.health.player.max),
    ));
}

#[cfg(test)]
mod tests {
    use super::{MAX_NAME_CHARS, sanitize_player_name};
    use crate::config::ServerGameplayConfig;
    use common::protocol::{
        BarrierKindId, HexColor, ItemType, KindDef, MapBootstrap, MapItems, MapLayout, MapSettings, PlayerBootstrap,
        PlayerId, PortalAccess, SInit, ServerMessage, WorldBootstrap,
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
            world: WorldBootstrap {
                gameplay: config.gameplay_bootstrap(),
                map: MapBootstrap {
                    layout: MapLayout::default(),
                    settings: MapSettings {
                        barrier_kinds: vec![
                            KindDef {
                                id: "lobby".to_owned(),
                                color: HexColor([0x22, 0xcc, 0x33]),
                                pressure_switch: Default::default(),
                            },
                            KindDef {
                                id: "basement".to_owned(),
                                color: HexColor([0xf0, 0xc0, 0x20]),
                                pressure_switch: Default::default(),
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
}

#[cfg(test)]
mod checkpoint_tests {
    use std::time::Duration;

    use super::handle_login_message;
    use crate::{
        config::{ActorRespawnScope, PlayerRespawnMode, ServerGameplayConfig},
        map::MapConfig,
        network::{CharacterQueries, ServerToClient, SharedWorld},
        players::{CheckpointId, PlayerCheckpoint, PlayerInfo, PlayerMap, respawn_tests::respawn_app},
        portals::{PortalAssignments, PortalMap},
        quests::{QuestBoard, QuestCatalog},
    };
    use bevy::{ecs::system::SystemState, prelude::*};
    use common::{
        map::Carriers,
        physics::{CollisionWorld, PortalSet},
        protocol::*,
    };
    use tokio::sync::mpsc::unbounded_channel;

    #[test]
    fn joining_inherits_shared_progress_and_respects_blocked_spawns_and_group_countdowns() {
        for (blocked, group_countdown) in [(false, false), (true, false), (false, true), (true, true)] {
            let mode = if group_countdown {
                PlayerRespawnMode::Group
            } else {
                PlayerRespawnMode::Individual
            };
            let mut app = respawn_app(mode, ActorRespawnScope::Dead);
            let checkpoint = Checkpoint {
                kind: CheckpointKind::GroupAny,
                carrier: CarrierId(1),
                level: 0,
                min_x: 0.0,
                max_x: 4.0,
                min_z: 0.0,
                max_z: 4.0,
                y: 0.0,
            };
            let mut layout = MapLayout {
                carriers: vec![Carrier {
                    parent: CarrierId::WORLD,
                    level: 0,
                    levels: 1,
                    from: Position {
                        x: 20.0,
                        y: 5.0,
                        z: 10.0,
                    },
                    to: Position {
                        x: 40.0,
                        y: 8.0,
                        z: 10.0,
                    },
                    travel_ticks: 30,
                    pause_ticks: 0,
                    phase_ticks: 0,
                }],
                ..default()
            };
            let floor = Floor {
                x1: 0.0,
                x2: 4.0,
                z1: 0.0,
                z2: 4.0,
                y: 0.0,
                thickness: 0.2,
                carrier: CarrierId(1),
                level: 0,
            };
            if !blocked {
                layout.floors.push(floor);
            }
            let mut carriers = Carriers::from_layout(&layout);
            carriers.advance(15);
            let mut collision = CollisionWorld::from_map_layout(&layout, &Default::default());
            collision.set_carrier_poses(&carriers);
            let settings = app.world().resource::<MapSettings>().clone();
            let gameplay = app.world().resource::<ServerGameplayConfig>().gameplay_bootstrap();
            app.insert_resource(WorldBootstrap {
                gameplay,
                map: MapBootstrap {
                    layout: layout.clone(),
                    settings: settings.clone(),
                    items: MapItems(Vec::new()),
                },
            });
            app.insert_resource(layout.clone())
                .insert_resource(collision)
                .insert_resource(carriers)
                .insert_resource(PortalAssignments::new(settings.portals))
                .init_resource::<PortalMap>()
                .init_resource::<PortalSet>();
            let catalog = QuestCatalog::from_quests(&[]);
            app.insert_resource(QuestBoard::from_catalog(&catalog))
                .insert_resource(catalog);
            app.world_mut().resource_mut::<MapConfig>().checkpoints = vec![checkpoint];
            app.world_mut().resource_mut::<PlayerMap>().shared_checkpoint = Some(PlayerCheckpoint {
                id: CheckpointId(0),
                facing: Vec3::X,
            });
            if group_countdown {
                let entity = app.world_mut().spawn_empty().id();
                let (channel, _) = unbounded_channel();
                let mut existing = PlayerInfo::new(entity, channel);
                existing.connection.logged_in = true;
                let mut players = app.world_mut().resource_mut::<PlayerMap>();
                players.insert(PlayerId(8), existing);
                assert!(players.begin_respawn(PlayerId(8), 2.0));
                players.take_resets();
                app.world_mut().despawn(entity);
            }
            let entity = app
                .world_mut()
                .spawn((
                    PlayerMarker,
                    PlayerId(9),
                    Position::default(),
                    PlayerMoveIntent::Idle,
                    FaceYaw(0.0),
                    Health(30.0),
                ))
                .id();
            let (tx, mut rx) = unbounded_channel();
            app.world_mut()
                .resource_mut::<PlayerMap>()
                .insert(PlayerId(9), PlayerInfo::new(entity, tx));
            let mut system: SystemState<(
                Commands,
                ResMut<PlayerMap>,
                SharedWorld,
                CharacterQueries,
                Res<QuestCatalog>,
                Res<QuestBoard>,
                ResMut<PortalAssignments>,
                ResMut<PortalMap>,
                ResMut<PortalSet>,
            )> = SystemState::new(app.world_mut());
            {
                let (
                    mut commands,
                    mut players,
                    world,
                    queries,
                    catalog,
                    board,
                    mut assignments,
                    mut portals,
                    mut portal_set,
                ) = system.get_mut(app.world_mut()).expect("login system resources missing");
                handle_login_message(
                    &mut commands,
                    entity,
                    PlayerId(9),
                    CLogin { name: "Player".into() },
                    &mut players,
                    &world,
                    &queries,
                    &catalog,
                    &board,
                    &mut assignments,
                    &mut portals,
                    &mut portal_set,
                );
            }
            system.apply(app.world_mut());
            assert!(matches!(
                rx.try_recv(),
                Ok(ServerToClient::Send(ServerMessage::Init(_)))
            ));
            let mut group_cues = 0;
            while let Ok(message) = rx.try_recv() {
                match message {
                    ServerToClient::Send(ServerMessage::PlayerDeath(death)) => {
                        assert_eq!(death.effect, PlayerDeathEffect::GroupRespawn);
                        group_cues += 1;
                    }
                    ServerToClient::Send(ServerMessage::CheckpointReached(_)) => {
                        panic!("login notified checkpoint entry")
                    }
                    _ => {}
                }
            }
            assert_eq!(group_cues, usize::from(group_countdown));
            assert!(app.world_mut().resource_mut::<PlayerMap>().take_resets().is_empty());
            let player = app
                .world()
                .resource::<PlayerMap>()
                .get(&PlayerId(9))
                .expect("joining player missing");
            assert_eq!(
                player.session.checkpoint.expect("shared checkpoint not inherited").id,
                CheckpointId(0)
            );
            assert_eq!(player.is_dead(), blocked || group_countdown);
            assert_eq!(player.session.score, 0);
            if group_countdown {
                app.world_mut()
                    .resource_mut::<Time>()
                    .advance_by(Duration::from_secs_f32(0.1));
                app.update();
                assert!(
                    app.world()
                        .resource::<PlayerMap>()
                        .get(&PlayerId(9))
                        .expect("joining player missing")
                        .is_dead()
                );
                app.world_mut()
                    .resource_mut::<Time>()
                    .advance_by(Duration::from_secs_f32(2.0));
                app.update();
                assert_eq!(
                    app.world()
                        .resource::<PlayerMap>()
                        .get(&PlayerId(9))
                        .expect("joining player missing")
                        .is_dead(),
                    blocked
                );
            }
            if blocked {
                assert!(app.world().get_entity(entity).is_err());
                layout.floors.push(floor);
                let mut collision = CollisionWorld::from_map_layout(&layout, &Default::default());
                collision.set_carrier_poses(app.world().resource::<Carriers>());
                app.insert_resource(collision);
                app.world_mut()
                    .resource_mut::<Time>()
                    .advance_by(Duration::from_secs_f32(0.1));
                app.update();
            }
            let player = app
                .world()
                .resource::<PlayerMap>()
                .get(&PlayerId(9))
                .expect("joining player missing");
            let body = player.entity().expect("joining player never spawned");
            assert_eq!(
                app.world().get::<Position>(body),
                Some(&Position {
                    x: 32.0,
                    y: 6.5,
                    z: 12.0
                })
            );
            assert_eq!(player.life.checkpoint_contact, Some(CheckpointId(0)));
        }
    }
}
