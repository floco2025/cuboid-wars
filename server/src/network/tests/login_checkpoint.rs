use std::time::Duration;

use super::handle_login_message;
use crate::{
    config::{ActorRespawnScope, PlayerRespawnMode, ServerGameplayConfig},
    map::MapConfig,
    network::{ServerToClient, SharedWorld, handlers::CharacterQueries},
    players::{CheckpointId, PlayerCheckpoint, PlayerInfo, PlayerMap, respawn_tests::respawn_app},
    portals::{PortalAssignments, PortalMap},
    quests::{QuestBoard, QuestCatalog},
};
use bevy::{ecs::system::SystemState, prelude::*};
use common::{map::Carriers, physics::CollisionWorld, protocol::*};
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
                switch: None,
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
        carriers.advance(15, &PlateState::default());
        let mut collision = CollisionWorld::from_map_layout(&layout, &Default::default());
        collision.set_carrier_poses(&carriers);
        let settings = app.world().resource::<MapSettings>().clone();
        let gameplay = app.world().resource::<ServerGameplayConfig>().gameplay_bootstrap();
        app.insert_resource(WorldBootstrap {
            network: Default::default(),
            gameplay,
            map: MapBootstrap {
                missile_air_grids: Vec::new(),
                layout: layout.clone(),
                settings: settings.clone(),
                items: MapItems(Vec::new()),
            },
        });
        app.insert_resource(layout.clone())
            .insert_resource(collision)
            .insert_resource(carriers)
            .insert_resource(PortalAssignments::new(settings.portals))
            .init_resource::<PortalMap>();
        let catalog = QuestCatalog::from_quests(&[]);
        app.insert_resource(QuestBoard::from_catalog(&catalog, None))
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
        )> = SystemState::new(app.world_mut());
        {
            let (mut commands, mut players, world, queries, catalog, board, mut assignments, mut portals) =
                system.get_mut(app.world_mut()).expect("login system resources missing");
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
            );
        }
        system.apply(app.world_mut());
        assert!(matches!(
            rx.try_recv(),
            Ok(ServerToClient::Send(ServerMessage::Init(_)))
        ));
        let mut group_cues = 0;
        let mut relocations = Vec::new();
        while let Ok(message) = rx.try_recv() {
            match message {
                ServerToClient::Send(ServerMessage::PlayerDeath(death)) => {
                    assert_eq!(death.effect, PlayerDeathEffect::GroupRespawn);
                    group_cues += 1;
                }
                ServerToClient::Send(ServerMessage::PlayerRelocated(relocation)) => relocations.push(relocation),
                ServerToClient::Send(ServerMessage::CheckpointReached(_)) => {
                    panic!("login notified checkpoint entry")
                }
                _ => {}
            }
        }
        assert_eq!(group_cues, usize::from(group_countdown));
        assert_eq!(relocations.len(), usize::from(!blocked && !group_countdown));
        for relocation in &relocations {
            assert_eq!(relocation.id, PlayerId(9));
            assert_eq!(relocation.player.generation, PlayerGeneration(0));
            assert_eq!(
                relocation.player.movement.pos,
                Position {
                    x: 32.0,
                    y: 6.5,
                    z: 12.0
                }
            );
            assert_eq!(
                relocation.player.health.0,
                app.world().resource::<ServerGameplayConfig>().combat.health.player.max
            );
        }
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
