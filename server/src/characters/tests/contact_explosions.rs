use super::*;
use crate::config::fixtures;
use crate::test_geometry::{WALL_HEIGHT, WALL_THICKNESS};
use common::protocol::{Barrier, BarrierId, BarrierKindId, CarrierId, MapLayout, Wall};

#[test]
fn touching_an_actor_does_not_detonate_it_during_peace() {
    let collision_world = CollisionWorld::from_map_layout(&MapLayout::default());
    let (player, mut actor, trigger_gap) = bodies();
    let mut world = World::new();
    actor.entity = world.spawn((ActorMarker, Health(100.0))).id();
    let entity = actor.entity;
    for (peaceful, expected_health) in [(true, 100.0), (false, 0.0)] {
        let mut state = world.query_filtered::<&mut Health, With<ActorMarker>>();
        detonate_actors_touching_players(
            &mut state.query_mut(&mut world),
            peaceful,
            &[player],
            &[(actor, trigger_gap)],
            &collision_world,
            &[],
        );
        assert_eq!(
            world.get::<Health>(entity).expect("actor health missing").0,
            expected_health
        );
    }
}

fn bodies() -> (CharacterBody, CharacterBody, f32) {
    let server = fixtures::server_config();
    let gameplay = server.gameplay_config();
    let player = CharacterBody {
        entity: Entity::from_bits(1),
        pos: Position {
            x: -0.3,
            y: 0.0,
            z: 0.0,
        },
        physics: gameplay.player.physics(),
    };
    let actor = CharacterBody {
        entity: Entity::from_bits(2),
        pos: Position { x: 0.3, y: 0.0, z: 0.0 },
        physics: gameplay.expect_actor("scuttler").physics(),
    };
    (
        player,
        actor,
        server
            .expect_actor("scuttler")
            .attack
            .contact_trigger_gap()
            .expect("scuttler contact attack missing from server gameplay config"),
    )
}

#[test]
fn nearby_player_triggers_contact_explosion_without_cover() {
    let (player, actor, distance) = bodies();
    let world = CollisionWorld::from_map_layout(&MapLayout::default());

    assert!(character_bodies_touch(&player, &actor, distance, &world, &[]));
}

#[test]
fn wall_blocks_contact_explosion() {
    let (player, actor, distance) = bodies();
    let layout = MapLayout {
        walls: vec![Wall {
            x1: 0.0,
            z1: -2.0,
            x2: 0.0,
            z2: 2.0,
            width: WALL_THICKNESS,
            level: 0,
            y: 0.0,
            height: WALL_HEIGHT,
            carrier: CarrierId::WORLD,
        }],
        ..default()
    };
    let world = CollisionWorld::from_map_layout(&layout);

    assert!(!character_bodies_touch(&player, &actor, distance, &world, &[]));
}

#[test]
fn vertically_separated_player_does_not_trigger_contact_explosion() {
    let (mut player, actor, distance) = bodies();
    player.pos.y = 3.0;
    let world = CollisionWorld::from_map_layout(&MapLayout::default());

    assert!(!character_bodies_touch(&player, &actor, distance, &world, &[]));
}

#[test]
fn closed_barrier_blocks_contact_detonation() {
    let (player, actor, distance) = bodies();
    let kind = BarrierKindId(0);
    let layout = MapLayout {
        barriers: vec![Barrier {
            id: Default::default(),

            switch: None,
            switch_inverted: false,

            x1: 0.0,
            z1: -2.0,
            x2: 0.0,
            z2: 2.0,
            y: 0.0,
            height: WALL_HEIGHT,
            width: 0.1,
            level: 0,
            levels: 1,
            kind,
            carrier: CarrierId::WORLD,
        }],
        ..default()
    };
    let world = CollisionWorld::from_map_layout(&layout);
    assert!(!character_bodies_touch(&player, &actor, distance, &world, &[]));
    assert!(character_bodies_touch(
        &player,
        &actor,
        distance,
        &world,
        &[BarrierId(u32::from(kind.0))]
    ));
}
