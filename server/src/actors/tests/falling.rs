use super::*;
use crate::{
    actors::{
        ActorInfo,
        test_kinds::{self, CONTACT},
    },
    config::FallDamageConfig,
    players::PlayerInfo,
};
use common::{
    config::ActorLocomotion,
    protocol::{CarrierId, PlayerId, ServerMessage},
};
use crossbeam_channel::{Receiver, unbounded};

const TEST_GRAVITY: f32 = 2.0;
const CONTACT_MAX_HEALTH: f32 = 150.0;

fn fall_app(safe_distance: f32, lethal_distance: f32) -> (App, Receiver<ServerMessage>) {
    let server = test_kinds::server_config();
    assert_eq!(server.combat.health.expect_actor(CONTACT).max, CONTACT_MAX_HEALTH);
    let mut settings = server.maps[&server.default_map].settings.clone();
    settings.movement.gravity = TEST_GRAVITY;
    let mut app = App::new();
    app.insert_resource(server)
        .insert_resource(settings)
        .insert_resource(FallDamageConfigs {
            player: FallDamageConfig {
                safe_distance: 0.0,
                lethal_distance: 1.0,
            },
            actor: FallDamageConfig {
                safe_distance,
                lethal_distance,
            },
        })
        .init_resource::<ActorMap>()
        .init_resource::<PlayerMap>()
        .init_resource::<PendingExplosions>()
        .add_systems(Update, actors_fall_damage_system);
    let observer = app.world_mut().spawn_empty().id();
    let (sender, receiver) = unbounded();
    let mut info = PlayerInfo::new(observer, sender);
    info.connection.logged_in = true;
    app.world_mut().resource_mut::<PlayerMap>().insert(PlayerId(1), info);
    (app, receiver)
}

// An actor that just landed from a `drop` metre fall under the test gravity.
fn spawn_landed_actor(app: &mut App, id: ActorId, locomotion: ActorLocomotion, health: f32, drop: f32) -> Entity {
    let mut character = test_kinds::kind(CONTACT).character;
    character.locomotion = locomotion;
    let entity = app
        .world_mut()
        .spawn((
            ActorMarker,
            id,
            Position {
                x: 3.0,
                y: 0.0,
                z: -2.0,
            },
            Health(health),
            ActorLanding((2.0 * TEST_GRAVITY * drop).sqrt()),
            ActorCharacter(character),
        ))
        .id();
    app.world_mut()
        .resource_mut::<ActorMap>()
        .insert(id, ActorInfo::new(entity, 0, CONTACT.into(), CarrierId::WORLD));
    entity
}

fn health_of(app: &App, entity: Entity) -> f32 {
    app.world().get::<Health>(entity).expect("actor health missing").0
}

#[test]
fn landing_damage_follows_the_map_thresholds_and_the_kind_max_health() {
    for (drop, expected_health) in [(0.0, 150.0), (4.0, 150.0), (4.04, 150.0), (8.0, 75.0), (10.0, 37.5)] {
        let (mut app, receiver) = fall_app(4.0, 12.0);
        let entity = spawn_landed_actor(&mut app, ActorId(1), ActorLocomotion::Ground, CONTACT_MAX_HEALTH, drop);
        app.update();
        assert!(
            (health_of(&app, entity) - expected_health).abs() < 0.001,
            "a {drop} m drop left {} health, expected {expected_health}",
            health_of(&app, entity)
        );
        assert!(app.world().resource::<ActorMap>().get(&ActorId(1)).is_some());
        assert!(receiver.try_recv().is_err());
    }
}

#[test]
fn a_lethal_landing_kills_the_actor_without_credit() {
    let (mut app, receiver) = fall_app(4.0, 12.0);
    let entity = spawn_landed_actor(&mut app, ActorId(7), ActorLocomotion::Ground, 40.0, 8.0);
    app.update();
    assert!(app.world().resource::<ActorMap>().get(&ActorId(7)).is_none());
    assert!(app.world().get_entity(entity).is_err());
    assert_eq!(app.world().resource::<PendingExplosions>().0.len(), 1);
    let death = loop {
        match receiver.try_recv().expect("no actor death reached the observer") {
            ServerMessage::ActorDeath(death) => break death,
            _ => continue,
        }
    };
    assert_eq!(death.id, ActorId(7));
    assert_eq!(death.killer, None);
    assert_eq!(
        death.pos,
        Position {
            x: 3.0,
            y: 0.0,
            z: -2.0
        }
    );
}

#[test]
fn flying_and_already_dead_actors_take_no_landing_damage() {
    let (mut app, receiver) = fall_app(4.0, 12.0);
    let flier = spawn_landed_actor(&mut app, ActorId(1), ActorLocomotion::Flying, CONTACT_MAX_HEALTH, 20.0);
    let shot_dead = spawn_landed_actor(&mut app, ActorId(2), ActorLocomotion::Ground, 0.0, 20.0);
    app.update();
    assert_eq!(health_of(&app, flier), CONTACT_MAX_HEALTH);
    assert_eq!(health_of(&app, shot_dead), 0.0);
    let actors = app.world().resource::<ActorMap>();
    assert!(actors.get(&ActorId(1)).is_some() && actors.get(&ActorId(2)).is_some());
    assert!(app.world().resource::<PendingExplosions>().0.is_empty());
    assert!(receiver.try_recv().is_err());
}
