use super::*;
use crate::{
    characters::PreviousTickPosition,
    players::{PlayerAnimationMotion, PlayerMap, PlayerMotionBundle, interpolate_remote_players_system},
    test_fixtures,
};
use bevy::ecs::system::{RunSystemOnce, SystemState};
use common::{
    physics::CharacterVerticalVelocity,
    protocol::{Health, PlayerGeneration, PlayerMove, PlayerMoveIntent, Position},
};

fn timing() -> SampleTiming {
    SampleTiming {
        delay_ticks: 2.0,
        interval_ticks: 1.0,
    }
}

fn player() -> Player {
    Player::new(
        "Player".into(),
        Position::default(),
        PlayerMoveIntent::Idle,
        0.0,
        0,
        Health(100.0),
    )
}

#[test]
fn relocation_is_applied_once_even_if_the_snapshot_arrives_first() {
    for (first_tick, later_tick) in [(10, 12), (12, 10)] {
        let mut world = World::new();
        let entity = world.spawn(Position::default()).id();
        let old = player();
        let mut info = PlayerInfo::from_snapshot(entity, &old, 0);
        let mut local = LocalPlayerInfo {
            is_dead: true,
            ..default()
        };
        local.reports.begin_crossing(old.movement.pos);
        let mut relocated = old.clone();
        relocated.generation = PlayerGeneration(1);
        relocated.movement.pos.x = 100.0;
        relocated.movement.face_yaw = 1.0;
        let mut state = SystemState::<Commands>::new(&mut world);
        assert!(place_player_body(
            &mut state.get_mut(&mut world).expect("commands unavailable"),
            &mut info,
            &mut local,
            true,
            first_tick,
            &relocated,
            &Carriers::default(),
            timing(),
        ));
        state.apply(&mut world);
        assert_eq!(
            *world.get::<Position>(entity).expect("position missing"),
            relocated.movement.pos
        );
        assert_eq!(
            world
                .get::<PreviousTickPosition>(entity)
                .expect("render anchor missing")
                .0,
            relocated.movement.pos
        );
        assert!(!local.is_dead);
        assert_eq!(local.stored_yaw, 1.0 + PI);
        world
            .entity_mut(entity)
            .insert((Position { x: 105.0, ..default() }, CharacterVerticalVelocity(7.0)));
        local.stored_yaw = 2.0;
        for update in [&relocated, &old] {
            assert!(!place_player_body(
                &mut state.get_mut(&mut world).expect("commands unavailable"),
                &mut info,
                &mut local,
                true,
                later_tick,
                update,
                &Carriers::default(),
                timing(),
            ));
            state.apply(&mut world);
            assert_eq!(world.get::<Position>(entity).expect("position missing").x, 105.0);
            assert_eq!(
                world
                    .get::<CharacterVerticalVelocity>(entity)
                    .expect("vertical velocity missing")
                    .0,
                7.0
            );
            assert_eq!(local.stored_yaw, 2.0);
        }
        assert_eq!(info.spawn_tick, first_tick);
    }
}

#[test]
fn relocation_discards_buffered_motion_from_the_previous_remote_body() {
    let mut world = World::new();
    world.insert_resource(Time::<()>::default());
    world.insert_resource(Time::<Fixed>::default());
    world.init_resource::<Carriers>();
    world.insert_resource(test_fixtures::map_settings());
    world.init_resource::<PlayerMap>();
    let old = player();
    let mut buffer = RemotePlayerMotion::new(old.movement, timing());
    buffer.push(PlayerMove {
        id: PlayerId(2),
        generation: old.generation,
        seq: 1,
        portal_crossing: 0,
        movement: old.movement,
    });
    let entity = world
        .spawn((
            PlayerId(2),
            old.movement.pos,
            PlayerMotionBundle::from(&old.movement),
            PlayerAnimationMotion::default(),
            buffer,
        ))
        .id();
    let mut info = PlayerInfo::from_snapshot(entity, &old, 1);
    let mut relocated = old.clone();
    relocated.generation = PlayerGeneration(1);
    relocated.movement.pos.x = 100.0;
    let mut state = SystemState::<Commands>::new(&mut world);
    assert!(place_player_body(
        &mut state.get_mut(&mut world).expect("commands missing"),
        &mut info,
        &mut LocalPlayerInfo::default(),
        false,
        2,
        &relocated,
        &Carriers::default(),
        timing(),
    ));
    state.apply(&mut world);
    world
        .run_system_once(interpolate_remote_players_system)
        .expect("remote interpolation failed");
    assert_eq!(world.get::<Position>(entity).expect("position missing").x, 100.0);
}

#[test]
fn a_snapshot_retires_only_bodies_placed_at_or_before_its_tick() {
    let mut players = PlayerMap::default();
    let stale = PlayerId(1);
    let fresh = PlayerId(2);
    let listed = PlayerId(3);
    for (id, spawn_tick) in [(stale, 10), (fresh, 21), (listed, 5)] {
        players.insert(
            id,
            PlayerInfo::from_snapshot(Entity::PLACEHOLDER, &player(), spawn_tick),
        );
    }
    let absent = absent_bodies(&players, 20, &[(listed, player())]);
    assert_eq!(absent.len(), 1);
    assert_eq!(absent[0].0, stale);
    assert_eq!(absent[0].1, PlayerGeneration(0));
    assert!(absent_bodies(&players, 21, &[]).iter().any(|(id, _, _)| *id == fresh));
}
