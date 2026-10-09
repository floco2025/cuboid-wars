use super::*;
use crate::{
    actors::test_kinds::{self, CONTACT},
    config::fixtures,
    test_geometry::{WALL_HEIGHT, WALL_THICKNESS},
};
use common::protocol::{Barrier, CarrierId, FieldId, MapLayout, Wall};

// A player and a contact actor side by side, and the actor's trigger gap.
fn bodies() -> (CharacterBody, CharacterBody, f32) {
    let player = CharacterBody {
        entity: Entity::from_bits(1),
        pos: Position {
            x: -0.3,
            y: 0.0,
            z: 0.0,
        },
        physics: fixtures::server_config().gameplay_config().player.physics(),
    };
    let actor = CharacterBody {
        entity: Entity::from_bits(2),
        pos: Position { x: 0.3, y: 0.0, z: 0.0 },
        physics: test_kinds::physics(CONTACT),
    };
    let trigger_gap = test_kinds::kind(CONTACT)
        .attack
        .contact_trigger_gap()
        .expect("contact kind has no trigger gap");
    (player, actor, trigger_gap)
}

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

#[test]
fn walls_and_closed_barriers_block_contact_detonation() {
    let (player, actor, distance) = bodies();
    let walled = CollisionWorld::from_map_layout(&MapLayout {
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
    });
    assert!(!character_bodies_touch(&player, &actor, distance, &walled, &[]));

    let kind = FieldId(0);
    let barred = CollisionWorld::from_map_layout(&MapLayout {
        barriers: vec![Barrier {
            x1: 0.0,
            z1: -2.0,
            x2: 0.0,
            z2: 2.0,
            y: 0.0,
            height: WALL_HEIGHT,
            width: 0.1,
            level: 0,
            levels: 1,
            field: kind,
            carrier: CarrierId::WORLD,
        }],
        ..default()
    });
    assert!(!character_bodies_touch(&player, &actor, distance, &barred, &[]));
    assert!(character_bodies_touch(&player, &actor, distance, &barred, &[kind]));
}

#[test]
fn vertically_separated_player_does_not_trigger_contact_explosion() {
    let (mut player, actor, distance) = bodies();
    player.pos.y = 3.0;
    let world = CollisionWorld::from_map_layout(&MapLayout::default());

    assert!(!character_bodies_touch(&player, &actor, distance, &world, &[]));
}
