use bevy::prelude::*;
use common::{
    map::Carriers,
    physics::{CharacterSupport, CollisionWorld},
    protocol::{
        Barrier, BarrierKindId, BarrierKindTable, Carrier, CarrierId, Checkpoint, CheckpointKind, FaceYaw, Floor,
        Health, MapLayout, PlateState, PlayerId, Position, ServerMessage,
    },
};

use super::{
    CheckpointId, PlayerCheckpoint, PlayerMap, PowerUpState, checkpoint_spawn_position,
    checkpoints::apply_checkpoint_entries,
    players_checkpoints_system,
    respawn_tests::{add_player, advance, kill, respawn_app},
};
use crate::{
    config::{ActorRespawnScope, PlayerRespawnMode, ServerGameplayConfig},
    network::ServerToClient,
    schedule::ServerSet,
};

fn checkpoint(min_x: f32) -> Checkpoint {
    Checkpoint {
        kind: CheckpointKind::Individual,
        carrier: CarrierId::WORLD,
        level: 0,
        min_x,
        max_x: min_x + 4.0,
        min_z: -2.0,
        max_z: 2.0,
        y: 0.0,
    }
}

fn floor(c: Checkpoint) -> Floor {
    Floor {
        x1: c.min_x,
        x2: c.max_x,
        z1: c.min_z,
        z2: c.max_z,
        y: c.y,
        thickness: 0.2,
        level: c.level,
        carrier: c.carrier,
    }
}

fn app(mode: PlayerRespawnMode) -> App {
    let mut app = respawn_app(mode, ActorRespawnScope::Dead);
    let checkpoints = vec![checkpoint(10.0), checkpoint(20.0)];
    let layout = MapLayout {
        floors: checkpoints.iter().copied().map(floor).collect(),
        checkpoints,
        ..default()
    };
    app.insert_resource(CollisionWorld::from_map_layout(&layout, &Default::default()));
    app.insert_resource(layout);
    app.add_systems(Update, players_checkpoints_system.in_set(ServerSet::Maintenance));
    app
}

fn stand(app: &mut App, id: PlayerId, pos: Position, support: CharacterSupport) {
    stand_facing(app, id, pos, support, 0.7);
}

fn stand_facing(app: &mut App, id: PlayerId, pos: Position, support: CharacterSupport, yaw: f32) {
    let entity = {
        let mut players = app.world_mut().resource_mut::<PlayerMap>();
        let player = players.get_mut(&id).expect("player missing");
        player.life.movement.support = support;
        player.entity().expect("player body missing")
    };
    app.world_mut().entity_mut(entity).insert((pos, FaceYaw(yaw)));
}

fn shared_facing(app: &App) -> Option<Vec3> {
    app.world()
        .resource::<PlayerMap>()
        .shared_checkpoint
        .map(|checkpoint| checkpoint.facing)
}

fn has_visit(app: &App, id: PlayerId, checkpoint: usize) -> bool {
    app.world()
        .resource::<PlayerMap>()
        .get(&id)
        .expect("player missing")
        .session
        .checkpoint_visits
        .contains_key(&CheckpointId(checkpoint))
}

fn saved(app: &App, id: PlayerId) -> Option<CheckpointId> {
    app.world()
        .resource::<PlayerMap>()
        .get(&id)
        .expect("player missing")
        .session
        .checkpoint
        .map(|c| c.id)
}

#[test]
fn only_grounded_players_activate_and_notifications_do_not_repeat() {
    let mut app = app(PlayerRespawnMode::Individual);
    let id = PlayerId(1);
    let (_, mut receiver) = add_player(&mut app, id);
    add_player(&mut app, PlayerId(2));
    for (y, support) in [
        (1.0, CharacterSupport::Airborne),
        (0.0, CharacterSupport::Airborne),
        (4.0, CharacterSupport::Ground),
        (0.0, CharacterSupport::Ladder),
    ] {
        stand(&mut app, id, Position { x: 12.0, y, z: 0.0 }, support);
        advance(&mut app, 0.0);
        assert!(saved(&app, id).is_none());
    }
    for (x, expected) in [(12.0, 0), (12.0, 0), (22.0, 1), (12.0, 0)] {
        stand(&mut app, id, Position { x, y: 0.0, z: 0.0 }, CharacterSupport::Ground);
        advance(&mut app, 0.0);
        assert_eq!(saved(&app, id), Some(CheckpointId(expected)));
    }
    assert!(saved(&app, PlayerId(2)).is_none());
    let mut notifications = 0;
    while let Ok(message) = receiver.try_recv() {
        if matches!(message, ServerToClient::Send(ServerMessage::CheckpointReached(_))) {
            notifications += 1;
        }
    }
    assert_eq!(notifications, 3);
    let delay = app.world().resource::<ServerGameplayConfig>().player.respawn_secs;
    app.world_mut().resource_mut::<PlayerMap>().disconnect(&id, delay);
    add_player(&mut app, id);
    assert!(saved(&app, id).is_none());
}

#[test]
fn deaths_preserve_checkpoints_clear_equipment_and_retry_blocked_group_or_individual_respawns() {
    for mode in [PlayerRespawnMode::Individual, PlayerRespawnMode::Group] {
        let mut app = app(mode);
        let id = PlayerId(1);
        let (_, mut receiver) = add_player(&mut app, id);
        stand(
            &mut app,
            id,
            Position {
                x: 12.0,
                y: 0.0,
                z: 0.0,
            },
            CharacterSupport::Ground,
        );
        advance(&mut app, 0.0);
        let full_health = app.world().resource::<ServerGameplayConfig>().combat.health.player.max;
        {
            let mut players = app.world_mut().resource_mut::<PlayerMap>();
            let player = players.get_mut(&id).expect("player missing");
            player.life.held_keys = vec![BarrierKindId(0)];
            player.life.power_ups.fill(PowerUpState::Permanent);
        }
        kill(&mut app, id);
        app.insert_resource(CollisionWorld::from_map_layout(
            &MapLayout::default(),
            &Default::default(),
        ));
        advance(&mut app, 2.1);
        assert!(
            app.world()
                .resource::<PlayerMap>()
                .get(&id)
                .expect("player missing")
                .is_dead()
        );
        let layout = MapLayout {
            floors: vec![floor(checkpoint(10.0))],
            ..default()
        };
        app.insert_resource(CollisionWorld::from_map_layout(&layout, &Default::default()));
        advance(&mut app, 0.1);
        let players = app.world().resource::<PlayerMap>();
        let player = players.get(&id).expect("player missing");
        let entity = player.entity().expect("blocked respawn did not retry");
        assert_eq!(player.life.missiles, 0);
        assert!(player.life.held_keys.is_empty());
        assert!(
            player
                .life
                .power_ups
                .iter()
                .all(|power| matches!(power, PowerUpState::Inactive))
        );
        assert_eq!(
            app.world().get::<Health>(entity).expect("health missing").0,
            full_health
        );
        assert_eq!(app.world().get::<Position>(entity).expect("position missing").x, 12.0);
        assert!((app.world().get::<FaceYaw>(entity).expect("facing missing").0 - 0.7).abs() < 1e-5);
        assert_eq!(saved(&app, id), Some(CheckpointId(0)));
        stand(
            &mut app,
            id,
            Position {
                x: 12.0,
                y: 0.0,
                z: 0.0,
            },
            CharacterSupport::Ground,
        );
        advance(&mut app, 0.0);
        let mut notifications = 0;
        while let Ok(message) = receiver.try_recv() {
            if matches!(message, ServerToClient::Send(ServerMessage::CheckpointReached(_))) {
                notifications += 1;
            }
        }
        assert_eq!(notifications, 1);
    }
}

#[test]
fn a_player_killed_at_a_checkpoint_does_not_activate_it() {
    let mut app = app(PlayerRespawnMode::Individual);
    let id = PlayerId(1);
    add_player(&mut app, id);
    stand(
        &mut app,
        id,
        Position {
            x: 12.0,
            y: 0.0,
            z: 0.0,
        },
        CharacterSupport::Ground,
    );
    kill(&mut app, id);
    advance(&mut app, 0.0);
    assert!(saved(&app, id).is_none());
}

#[test]
fn checkpoint_spawns_follow_carriers_and_avoid_players_and_barriers() {
    let physics = ServerGameplayConfig::load_default()
        .expect("gameplay config rejected")
        .gameplay_config()
        .player
        .physics();
    let mut c = checkpoint(0.0);
    c.carrier = CarrierId(1);
    let mut layout = MapLayout {
        floors: vec![floor(c)],
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
    let mut carriers = Carriers::from_layout(&layout);
    let pose = carriers.pose(c.carrier);
    let mut world = CollisionWorld::from_map_layout(&layout, &Default::default());
    let center = checkpoint_spawn_position(&c, &pose, &world, &[], physics).expect("clear checkpoint rejected");
    assert_eq!(
        center,
        Position {
            x: 22.0,
            y: 5.0,
            z: 10.0
        }
    );
    let other =
        checkpoint_spawn_position(&c, &pose, &world, &[center], physics).expect("unoccupied checkpoint area rejected");
    assert!(other.horizontal_distance_sq(&center) >= physics.movement_collider.diameter.powi(2));
    carriers.advance(15, &PlateState::default());
    world.set_carrier_poses(&carriers);
    let moved_pose = carriers.pose(c.carrier);
    let moved = checkpoint_spawn_position(&c, &moved_pose, &world, &[], physics).expect("moving checkpoint rejected");
    assert_eq!(
        moved,
        Position {
            x: 32.0,
            y: 6.5,
            z: 10.0
        }
    );
    layout.barriers.push(Barrier {
        x1: 0.0,
        z1: 0.0,
        x2: 4.0,
        z2: 0.0,
        y: 0.0,
        height: 3.0,
        width: 8.0,
        kind: BarrierKindId(0),
        level: 0,
        levels: 1,
        carrier: c.carrier,
    });
    let kinds = BarrierKindTable::from_ids(vec!["gate".into()]).expect("barrier catalog rejected");
    let world = CollisionWorld::from_map_layout(&layout, &kinds);
    assert!(checkpoint_spawn_position(&c, &pose, &world, &[], physics).is_none());
}

fn entries(app: &mut App, entries: &[(u32, usize)]) {
    let checkpoints = app.world().resource::<MapLayout>().checkpoints.clone();
    let entered = entries
        .iter()
        .map(|&(player, checkpoint)| {
            (
                PlayerId(player),
                PlayerCheckpoint {
                    id: CheckpointId(checkpoint),
                    facing: Vec3::new(player as f32, 0.0, 1.0).normalize(),
                },
            )
        })
        .collect();
    apply_checkpoint_entries(&mut app.world_mut().resource_mut::<PlayerMap>(), &checkpoints, entered);
}

fn disconnect(app: &mut App, id: u32) {
    let info = app
        .world_mut()
        .resource_mut::<PlayerMap>()
        .disconnect(&PlayerId(id), 2.0)
        .expect("departing player missing");
    if let Some(entity) = info.entity() {
        app.world_mut().despawn(entity);
    }
}

#[test]
fn shared_checkpoints_work_with_both_respawn_policies() {
    for mode in [PlayerRespawnMode::Individual, PlayerRespawnMode::Group] {
        for kind in [CheckpointKind::GroupAny, CheckpointKind::GroupAll] {
            let mut app = app(mode);
            app.world_mut().resource_mut::<MapLayout>().checkpoints[1].kind = kind;
            add_player(&mut app, PlayerId(1));
            add_player(&mut app, PlayerId(2));
            entries(&mut app, &[(1, 1), (2, 1)]);
            for id in [PlayerId(1), PlayerId(2)] {
                assert_eq!(saved(&app, id), Some(CheckpointId(1)));
            }
            kill(&mut app, PlayerId(1));
            advance(&mut app, 2.1);
            for id in [PlayerId(1), PlayerId(2)] {
                assert_eq!(saved(&app, id), Some(CheckpointId(1)));
                let player = app.world().resource::<PlayerMap>().get(&id).expect("player missing");
                let pos = app
                    .world()
                    .get::<Position>(player.entity().expect("respawn missing"))
                    .expect("position missing");
                if id == PlayerId(1) || mode == PlayerRespawnMode::Group {
                    assert!(pos.x >= 20.0 && pos.x < 24.0);
                }
            }
        }
    }
}

#[test]
fn group_all_visits_survive_death_and_membership_changes() {
    let mut app = app(PlayerRespawnMode::Individual);
    app.world_mut().resource_mut::<MapLayout>().checkpoints[1].kind = CheckpointKind::GroupAll;
    let (_, mut first) = add_player(&mut app, PlayerId(1));
    let (_, mut second) = add_player(&mut app, PlayerId(2));
    entries(&mut app, &[(1, 1)]);
    assert!(saved(&app, PlayerId(1)).is_none());
    assert!(first.try_recv().is_err());
    kill(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(3));
    entries(&mut app, &[(2, 1)]);
    assert!(saved(&app, PlayerId(2)).is_none());
    disconnect(&mut app, 3);
    entries(&mut app, &[]);
    for id in [PlayerId(1), PlayerId(2)] {
        assert_eq!(saved(&app, id), Some(CheckpointId(1)));
        let player = app.world().resource::<PlayerMap>().get(&id).expect("player missing");
        assert!(player.session.checkpoint_visits.is_empty());
        assert_eq!(
            player.session.checkpoint.expect("saved checkpoint missing").facing,
            Vec3::new(1.0, 0.0, 1.0).normalize()
        );
    }
    let mut cues = 0;
    while let Ok(message) = second.try_recv() {
        if matches!(message, ServerToClient::Send(ServerMessage::CheckpointReached(_))) {
            cues += 1;
        }
    }
    assert_eq!(cues, 1);
    assert!(
        app.world()
            .resource::<PlayerMap>()
            .get(&PlayerId(1))
            .expect("player missing")
            .is_dead()
    );
    advance(&mut app, 2.1);
    let player = app
        .world()
        .resource::<PlayerMap>()
        .get(&PlayerId(1))
        .expect("player missing");
    assert!(
        app.world()
            .get::<Position>(player.entity().expect("respawn missing"))
            .expect("position missing")
            .x
            >= 20.0
    );
}

#[test]
fn a_shared_activation_clears_other_partial_visits_and_empty_sessions_reset() {
    let mut app = app(PlayerRespawnMode::Individual);
    app.world_mut().resource_mut::<MapLayout>().checkpoints[0].kind = CheckpointKind::GroupAll;
    app.world_mut().resource_mut::<MapLayout>().checkpoints[1].kind = CheckpointKind::GroupAny;
    add_player(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(2));
    entries(&mut app, &[(1, 0)]);
    entries(&mut app, &[(2, 1)]);
    entries(&mut app, &[(2, 0)]);
    assert_eq!(saved(&app, PlayerId(1)), Some(CheckpointId(1)));
    entries(&mut app, &[(1, 0)]);
    assert_eq!(saved(&app, PlayerId(2)), Some(CheckpointId(0)));
    disconnect(&mut app, 1);
    assert!(app.world().resource::<PlayerMap>().shared_checkpoint.is_some());
    disconnect(&mut app, 2);
    assert!(app.world().resource::<PlayerMap>().shared_checkpoint.is_none());
}

#[test]
fn shared_activations_win_simultaneous_individual_entries_once_in_map_order() {
    let mut app = app(PlayerRespawnMode::Individual);
    let mut third = checkpoint(30.0);
    third.kind = CheckpointKind::GroupAny;
    app.world_mut().resource_mut::<MapLayout>().checkpoints.push(third);
    app.world_mut().resource_mut::<MapLayout>().checkpoints[0].kind = CheckpointKind::GroupAny;
    let (_, mut rx) = add_player(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(2));
    entries(&mut app, &[(1, 1), (2, 2), (2, 0)]);
    assert_eq!(saved(&app, PlayerId(1)), Some(CheckpointId(0)));
    assert_eq!(saved(&app, PlayerId(2)), Some(CheckpointId(0)));
    assert!(matches!(
        rx.try_recv(),
        Ok(ServerToClient::Send(ServerMessage::CheckpointReached(_)))
    ));
    assert!(rx.try_recv().is_err());
}

#[test]
fn stationary_or_respawning_players_do_not_overwrite_teammates_individual_progress() {
    let mut app = app(PlayerRespawnMode::Individual);
    app.world_mut().resource_mut::<MapLayout>().checkpoints[1].kind = CheckpointKind::GroupAny;
    add_player(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(2));
    stand(
        &mut app,
        PlayerId(1),
        Position {
            x: 22.0,
            y: 0.0,
            z: 0.0,
        },
        CharacterSupport::Ground,
    );
    advance(&mut app, 0.0);
    stand(
        &mut app,
        PlayerId(2),
        Position {
            x: 12.0,
            y: 0.0,
            z: 0.0,
        },
        CharacterSupport::Ground,
    );
    advance(&mut app, 0.0);
    advance(&mut app, 0.0);
    assert_eq!(saved(&app, PlayerId(2)), Some(CheckpointId(0)));
    kill(&mut app, PlayerId(1));
    advance(&mut app, 2.1);
    stand(
        &mut app,
        PlayerId(1),
        Position {
            x: 22.0,
            y: 0.0,
            z: 0.0,
        },
        CharacterSupport::Ground,
    );
    advance(&mut app, 0.0);
    assert_eq!(saved(&app, PlayerId(2)), Some(CheckpointId(0)));
    stand(
        &mut app,
        PlayerId(1),
        Position {
            x: 18.0,
            y: 0.0,
            z: 0.0,
        },
        CharacterSupport::Airborne,
    );
    advance(&mut app, 0.0);
    stand(
        &mut app,
        PlayerId(1),
        Position {
            x: 22.0,
            y: 0.0,
            z: 0.0,
        },
        CharacterSupport::Ground,
    );
    advance(&mut app, 0.0);
    assert_eq!(
        saved(&app, PlayerId(2)),
        Some(CheckpointId(0)),
        "returning to the active shared checkpoint does not either"
    );
}

#[test]
fn re_entering_the_active_shared_checkpoint_changes_nothing() {
    let mut app = app(PlayerRespawnMode::Individual);
    app.world_mut().resource_mut::<MapLayout>().checkpoints[0].kind = CheckpointKind::GroupAll;
    app.world_mut().resource_mut::<MapLayout>().checkpoints[1].kind = CheckpointKind::GroupAny;
    add_player(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(2));
    let inside = Position {
        x: 22.0,
        y: 0.0,
        z: 0.0,
    };
    stand_facing(&mut app, PlayerId(1), inside, CharacterSupport::Ground, 0.7);
    advance(&mut app, 0.0);
    let facing = shared_facing(&app).expect("shared checkpoint not activated");
    stand(
        &mut app,
        PlayerId(2),
        Position {
            x: 12.0,
            y: 0.0,
            z: 0.0,
        },
        CharacterSupport::Ground,
    );
    advance(&mut app, 0.0);
    assert!(has_visit(&app, PlayerId(2), 0), "a partial group visit");

    // A jump in place: above the seeded contact's band the airborne tick drops
    // it, and the landing re-enters, facing another way.
    stand_facing(
        &mut app,
        PlayerId(1),
        Position { y: 1.0, ..inside },
        CharacterSupport::Airborne,
        2.0,
    );
    advance(&mut app, 0.0);
    stand_facing(&mut app, PlayerId(1), inside, CharacterSupport::Ground, 2.0);
    advance(&mut app, 0.0);
    assert_eq!(shared_facing(&app), Some(facing), "the saved facing stands");
    assert!(has_visit(&app, PlayerId(2), 0), "the partial visit stands");
    assert_eq!(saved(&app, PlayerId(2)), Some(CheckpointId(1)));

    // Leaving and returning is the same re-entry.
    stand_facing(
        &mut app,
        PlayerId(1),
        Position {
            x: 18.0,
            y: 0.0,
            z: 0.0,
        },
        CharacterSupport::Ground,
        2.0,
    );
    advance(&mut app, 0.0);
    stand_facing(&mut app, PlayerId(1), inside, CharacterSupport::Ground, 2.0);
    advance(&mut app, 0.0);
    assert_eq!(shared_facing(&app), Some(facing));
    assert!(has_visit(&app, PlayerId(2), 0));

    // Another shared checkpoint still activates once everyone has visited it.
    stand(
        &mut app,
        PlayerId(1),
        Position {
            x: 12.0,
            y: 0.0,
            z: 0.0,
        },
        CharacterSupport::Ground,
    );
    advance(&mut app, 0.0);
    assert_eq!(saved(&app, PlayerId(1)), Some(CheckpointId(0)));
    assert_eq!(saved(&app, PlayerId(2)), Some(CheckpointId(0)));
}

#[test]
fn a_re_entry_of_the_active_checkpoint_is_not_deferred() {
    let mut app = app(PlayerRespawnMode::Individual);
    app.world_mut().resource_mut::<MapLayout>().checkpoints[0].kind = CheckpointKind::GroupAny;
    app.world_mut().resource_mut::<MapLayout>().checkpoints[1].kind = CheckpointKind::GroupAny;
    add_player(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(2));
    let inside = Position {
        x: 22.0,
        y: 0.0,
        z: 0.0,
    };
    stand(&mut app, PlayerId(1), inside, CharacterSupport::Ground);
    advance(&mut app, 0.0);
    assert_eq!(saved(&app, PlayerId(2)), Some(CheckpointId(1)));

    // One tick: player 1 lands again in the active checkpoint (a jump above
    // the seeded contact's band) while player 2 enters the other one.
    stand(
        &mut app,
        PlayerId(1),
        Position { y: 1.0, ..inside },
        CharacterSupport::Airborne,
    );
    advance(&mut app, 0.0);
    assert!(
        app.world()
            .resource::<PlayerMap>()
            .get(&PlayerId(1))
            .expect("player missing")
            .life
            .checkpoint_contact
            .is_none(),
        "the jump must drop the contact for the landing to count as a re-entry"
    );
    stand(&mut app, PlayerId(1), inside, CharacterSupport::Ground);
    stand(
        &mut app,
        PlayerId(2),
        Position {
            x: 12.0,
            y: 0.0,
            z: 0.0,
        },
        CharacterSupport::Ground,
    );
    advance(&mut app, 0.0);
    assert_eq!(saved(&app, PlayerId(1)), Some(CheckpointId(0)));
    for _ in 0..3 {
        advance(&mut app, 0.0);
        assert_eq!(
            saved(&app, PlayerId(1)),
            Some(CheckpointId(0)),
            "nobody moved, nothing rolls back"
        );
        assert_eq!(saved(&app, PlayerId(2)), Some(CheckpointId(0)));
    }
}

#[test]
fn simultaneous_shared_entries_activate_on_consecutive_ticks() {
    let mut app = app(PlayerRespawnMode::Individual);
    app.world_mut().resource_mut::<MapLayout>().checkpoints[0].kind = CheckpointKind::GroupAny;
    app.world_mut().resource_mut::<MapLayout>().checkpoints[1].kind = CheckpointKind::GroupAny;
    let (_, mut rx) = add_player(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(2));
    stand(
        &mut app,
        PlayerId(1),
        Position {
            x: 12.0,
            y: 0.0,
            z: 0.0,
        },
        CharacterSupport::Ground,
    );
    stand(
        &mut app,
        PlayerId(2),
        Position {
            x: 22.0,
            y: 0.0,
            z: 0.0,
        },
        CharacterSupport::Ground,
    );
    advance(&mut app, 0.0);
    assert_eq!(
        saved(&app, PlayerId(1)),
        Some(CheckpointId(0)),
        "map order wins the tick"
    );
    assert_eq!(saved(&app, PlayerId(2)), Some(CheckpointId(0)));

    advance(&mut app, 0.0);
    assert_eq!(
        saved(&app, PlayerId(1)),
        Some(CheckpointId(1)),
        "the other entry follows"
    );
    assert_eq!(saved(&app, PlayerId(2)), Some(CheckpointId(1)));
    let mut cues = 0;
    while let Ok(message) = rx.try_recv() {
        if matches!(message, ServerToClient::Send(ServerMessage::CheckpointReached(_))) {
            cues += 1;
        }
    }
    assert_eq!(cues, 2);
    advance(&mut app, 0.0);
    assert_eq!(
        saved(&app, PlayerId(2)),
        Some(CheckpointId(1)),
        "and nothing flips back"
    );
}
