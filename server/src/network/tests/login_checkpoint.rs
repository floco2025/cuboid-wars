use std::time::Duration;

use super::handle_login_message;
use crate::{
    config::{ActorRespawnScope, PlayerRespawnMode, ServerGameplayConfig},
    map::{CarrierGrid, CellGrid, EdgeGrid, LevelGrid, MapConfig},
    network::{SharedWorld, handlers::CharacterQueries},
    players::{CheckpointId, LoginStart, PlayerCheckpoint, PlayerInfo, PlayerMap, respawn_tests::respawn_app},
    portals::{PortalAssignments, PortalMap},
    quests::{QuestBoard, QuestCatalog},
    test_geometry::geometry,
};
use bevy::{ecs::system::SystemState, prelude::*};
use common::{celestial::CelestialClockAnchor, map::Carriers, physics::CollisionWorld, protocol::*};
use crossbeam_channel::unbounded;

#[test]
fn joining_inherits_shared_progress_and_respects_blocked_spawns_and_group_countdowns() {
    for (blocked, group_countdown) in [(false, false), (true, false), (false, true), (true, true)] {
        let mode = if group_countdown {
            PlayerRespawnMode::Group
        } else {
            PlayerRespawnMode::Individual
        };
        let mut app = respawn_app(mode, ActorRespawnScope::Dead);
        let geometry = geometry(2, 2);
        let checkpoint = Checkpoint {
            kind: CheckpointKind::GroupAny,
            number: 1,
            carrier: CarrierId(1),
            level: 0,
            cols: [1, 2],
            rows: [1, 2],
            min_x: geometry.cell_to_world_x(1),
            max_x: geometry.cell_to_world_x(2),
            min_z: geometry.cell_to_world_z(1),
            max_z: geometry.cell_to_world_z(2),
            y: 0.0,
        };
        // The checkpoint's cell on the carrier's grid; a ramp flag there blocks its spawns.
        let mut cells = CellGrid::new(2, 2);
        cells.rows[1][1].has_floor = true;
        cells.rows[1][1].has_ramp = blocked;
        app.world_mut().resource_mut::<MapConfig>().grids.push(CarrierGrid::new(
            CarrierId(1),
            geometry,
            vec![LevelGrid {
                cells,
                edges: EdgeGrid::new(2, 2),
                barrier_edges: EdgeGrid::new(2, 2),
            }],
        ));
        let mut layout = MapLayout {
            carriers: vec![Carrier {
                motion: Default::default(),
                switch_inverted: false,

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
        layout.floors.push(Floor {
            x1: checkpoint.min_x,
            x2: checkpoint.max_x,
            z1: checkpoint.min_z,
            z2: checkpoint.max_z,
            y: 0.0,
            thickness: 0.2,
            carrier: CarrierId(1),
            level: 0,
        });
        layout.checkpoints.push(checkpoint);
        let mut carriers = Carriers::from_layout(&layout);
        carriers.advance(15, &SwitchState::default());
        let mut collision = CollisionWorld::from_map_layout(&layout);
        collision.set_carrier_poses(&carriers);
        let settings = app.world().resource::<MapSettings>().clone();
        let gameplay = app.world().resource::<ServerGameplayConfig>().gameplay_bootstrap();
        let celestial = app.world().resource::<ServerGameplayConfig>().cycles.celestial;
        app.insert_resource(WorldBootstrap {
            network: Default::default(),
            celestial,
            gameplay,
            map: MapBootstrap {
                missile_air_grids: Vec::new(),
                layout: layout.clone(),
                settings: settings.clone(),
                items: MapItems(Vec::new()),
            },
        });
        app.insert_resource(CelestialClockAnchor::initial(&settings.celestial, 0));
        app.insert_resource(layout.clone())
            .insert_resource(collision)
            .insert_resource(carriers)
            .insert_resource(PortalAssignments::new(settings.portals))
            .init_resource::<PortalMap>();
        let catalog = QuestCatalog::from_quests(&[]);
        app.insert_resource(QuestBoard::from_catalog(&catalog, None))
            .insert_resource(catalog);
        app.world_mut().resource_mut::<PlayerMap>().shared_checkpoint = Some(PlayerCheckpoint {
            id: CheckpointId(0),
            facing: Vec3::X,
        });
        if group_countdown {
            let entity = app.world_mut().spawn_empty().id();
            let (channel, _) = unbounded();
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
        let (tx, rx) = unbounded();
        app.world_mut()
            .resource_mut::<PlayerMap>()
            .insert(PlayerId(9), PlayerInfo::new(entity, tx));
        let mut system: SystemState<(
            Commands,
            ResMut<PlayerMap>,
            SharedWorld,
            Res<CelestialClockAnchor>,
            CharacterQueries,
            Res<QuestCatalog>,
            Res<QuestBoard>,
            ResMut<PortalAssignments>,
            ResMut<PortalMap>,
        )> = SystemState::new(app.world_mut());
        {
            let (
                mut commands,
                mut players,
                world,
                celestial_clock,
                queries,
                catalog,
                board,
                mut assignments,
                mut portals,
            ) = system.get_mut(app.world_mut()).expect("login system resources missing");
            handle_login_message(
                &mut commands,
                entity,
                PlayerId(9),
                CLogin { name: "Player".into() },
                LoginStart::default(),
                &mut players,
                &world,
                &celestial_clock,
                &queries,
                &catalog,
                &board,
                &mut assignments,
                &mut portals,
            );
        }
        system.apply(app.world_mut());
        assert!(matches!(rx.try_recv(), Ok(ServerMessage::Init(_))));
        let mut group_cues = 0;
        let mut relocations = Vec::new();
        while let Ok(message) = rx.try_recv() {
            match message {
                ServerMessage::PlayerDeath(death) => {
                    assert_eq!(death.effect, PlayerDeathEffect::GroupRespawn);
                    group_cues += 1;
                }
                ServerMessage::PlayerRelocated(relocation) => relocations.push(relocation),
                ServerMessage::CheckpointReached(_) => {
                    panic!("login notified checkpoint entry")
                }
                _ => {}
            }
        }
        assert_eq!(group_cues, usize::from(group_countdown));
        // A blocked checkpoint places the joiner in a spawn zone instead of leaving it bodiless.
        assert_eq!(relocations.len(), usize::from(!group_countdown));
        let in_checkpoint = |app: &App, pos: &Position| {
            let local = app
                .world()
                .resource::<Carriers>()
                .pose(CarrierId(1))
                .inverse_transform_position(pos);
            (geometry.cell_to_world_x(1)..geometry.cell_to_world_x(2)).contains(&local.x)
                && (geometry.cell_to_world_z(1)..geometry.cell_to_world_z(2)).contains(&local.z)
                && local.y.abs() < 1e-3
        };
        for relocation in &relocations {
            assert_eq!(relocation.id, PlayerId(9));
            assert_eq!(relocation.player.generation, PlayerGeneration(0));
            assert_eq!(in_checkpoint(&app, &relocation.player.movement.pos), !blocked);
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
        assert_eq!(player.is_dead(), group_countdown);
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
            assert_eq!(app.world().get_entity(entity).is_err(), group_countdown);
            app.world_mut().resource_mut::<MapConfig>().grids[1].levels[0]
                .cells
                .rows[1][1]
                .has_ramp = false;
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
        let pos = app.world().get::<Position>(body).expect("body position missing");
        if blocked && !group_countdown {
            // Already alive in a spawn zone; clearing the checkpoint moves nobody.
            assert!(!in_checkpoint(&app, pos));
            assert_eq!(player.life.checkpoint_contact, None);
        } else {
            assert!(in_checkpoint(&app, pos));
            assert_eq!(player.life.checkpoint_contact, Some(CheckpointId(0)));
        }
    }
}
