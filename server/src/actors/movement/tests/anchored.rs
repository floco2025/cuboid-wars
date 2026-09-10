use super::super::{ActorMovementQuery, apply_actor_moves, plan_actor_moves};
use crate::{
    actors::{
        ActorMap, PendingActorSpawn, PendingActorSpawns, actors_pending_spawn_system,
        test_kinds::{self, IMMOVABLE},
    },
    players::PlayerMap,
};
use bevy::prelude::*;
use common::{
    config::ActorMovementConfig,
    map::Carriers,
    physics::{CharacterVerticalVelocity, CollisionWorld, KnockbackVelocity},
    protocol::{
        ActorAnchor, ActorId, ActorMoveIntent, Carrier, CarrierId, MapLayout, MapSettings, PlateState, Position,
        ServerTick,
    },
};

fn step(
    world: Res<CollisionWorld>,
    settings: Res<MapSettings>,
    plates: Res<PlateState>,
    carriers: Res<Carriers>,
    actors: Res<ActorMap>,
    mut query: ActorMovementQuery,
) {
    let starts = query
        .iter()
        .map(|(entity, _, _, pos, _, _, _, _, _, _, character)| (entity, *pos, character.0.physics()))
        .collect::<Vec<_>>();
    let mut planned = Vec::new();
    plan_actor_moves(
        1.0 / 30.0,
        &world,
        &settings,
        &plates,
        &carriers,
        &actors,
        &starts,
        &mut query,
        &mut planned,
    );
    apply_actor_moves(&mut query, &actors, &planned);
}

#[test]
fn turret_stays_at_carrier_anchor_despite_gravity_and_knockback() {
    let server = test_kinds::server_config();
    let settings = server.maps[&server.default_map].settings.clone();
    let layout = MapLayout {
        carriers: vec![Carrier {
            parent: CarrierId::WORLD,
            level: 0,
            levels: 1,
            from: Position::default(),
            to: Position {
                x: 12.0,
                y: 4.0,
                z: 0.0,
            },
            travel_ticks: 30,
            pause_ticks: 2,
            phase_ticks: 0,
            switch: None,
        }],
        ..Default::default()
    };
    let anchor = ActorAnchor {
        carrier: CarrierId(1),
        pos: Position { x: 1.0, y: 9.0, z: 2.0 },
    };
    let mut app = App::new();
    app.insert_resource(settings)
        .insert_resource(server)
        .insert_resource(Carriers::from_layout(&layout))
        .insert_resource(CollisionWorld::from_map_layout(&layout, &Default::default()))
        .init_resource::<ActorMap>()
        .init_resource::<PlayerMap>()
        .init_resource::<PlateState>()
        .init_resource::<ServerTick>()
        .insert_resource(PendingActorSpawns(vec![PendingActorSpawn {
            actor_id: ActorId(1),
            zone_idx: 0,
            kind: IMMOVABLE.into(),
            carrier: anchor.carrier,
            pos: anchor.pos,
            face_yaw: 0.0,
            reserved_tick: 0,
            due_tick: 0,
        }]))
        .add_systems(Update, (actors_pending_spawn_system, step).chain());
    let id = ActorId(1);
    app.update();
    let actor = app.world().resource::<ActorMap>().get(&id).expect("turret missing");
    let entity = actor.entity;
    assert_eq!(actor.anchor, Some(anchor));
    assert!(app.world().get::<ActorMovementConfig>(entity).is_none());
    app.world_mut().entity_mut(entity).insert((
        Position::default(),
        CharacterVerticalVelocity(10.0),
        KnockbackVelocity(Vec3::new(20.0, 0.0, 20.0)),
    ));
    for tick in 0..130 {
        app.world_mut()
            .resource_mut::<Carriers>()
            .advance(tick, &PlateState::default());
        app.update();
        let expected = anchor.world_position(app.world().resource::<Carriers>());
        assert_eq!(
            *app.world().get::<Position>(entity).expect("turret position missing"),
            expected
        );
        assert_eq!(
            app.world()
                .get::<CharacterVerticalVelocity>(entity)
                .expect("turret velocity missing")
                .0,
            0.0
        );
        assert_eq!(
            *app.world()
                .get::<ActorMoveIntent>(entity)
                .expect("turret intent missing"),
            ActorMoveIntent::Idle
        );
    }
}
