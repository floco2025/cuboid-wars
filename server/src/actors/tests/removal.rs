use super::*;
use crate::{
    actors::{ActorInfo, test_kinds},
    combat::PendingExplosion,
    map::{ActorSpawnZone, CheckpointResponse, MapConfig},
    players::{CheckpointId, PlayerCheckpoint, PlayerInfo},
    test_geometry::geometry,
};
use common::protocol::{CarrierId, Checkpoint, CheckpointKind, MapLayout, ServerMessage};
use crossbeam_channel::{Receiver, unbounded};

fn removal_app(zone: Option<ActorSpawnZone>) -> App {
    let mut map = MapConfig::for_grid(Vec::new(), geometry(1, 1));
    map.actor_spawn_zones.extend(zone);
    let checkpoints = [1, 2]
        .map(|number| Checkpoint {
            kind: CheckpointKind::Individual,
            number,
            carrier: CarrierId::WORLD,
            level: 0,
            cols: [0, 1],
            rows: [0, 1],
            min_x: 0.0,
            max_x: 1.0,
            min_z: 0.0,
            max_z: 1.0,
            y: 0.0,
        })
        .to_vec();
    let mut app = App::new();
    app.init_resource::<ActorMap>()
        .init_resource::<PlayerMap>()
        .init_resource::<PendingExplosions>()
        .insert_resource(test_kinds::server_config())
        .insert_resource(map)
        .insert_resource(MapLayout {
            checkpoints,
            ..default()
        })
        .add_systems(Update, actors_removal_system);
    app
}

fn spawn_actor(app: &mut App, id: ActorId, pos: Position) -> Entity {
    let entity = app
        .world_mut()
        .spawn((id, ActorMarker, pos, Health(100.0), ActorCrushed::default()))
        .id();
    app.world_mut()
        .resource_mut::<ActorMap>()
        .insert(id, ActorInfo::new(entity, 0, test_kinds::CONTACT.into(), CarrierId(1)));
    entity
}

// A logged-in player whose saved checkpoint is `checkpoint`, and its channel.
fn add_player_at(app: &mut App, id: PlayerId, checkpoint: Option<usize>) -> Receiver<ServerMessage> {
    let entity = app.world_mut().spawn_empty().id();
    let (tx, rx) = unbounded();
    let mut info = PlayerInfo::new(entity, tx);
    info.connection.logged_in = true;
    set_checkpoint(&mut info, checkpoint);
    app.world_mut().resource_mut::<PlayerMap>().insert(id, info);
    rx
}

fn set_checkpoint(info: &mut PlayerInfo, checkpoint: Option<usize>) {
    info.session.checkpoint = checkpoint.map(|index| PlayerCheckpoint {
        id: CheckpointId(index),
        facing: Vec3::X,
    });
}

fn zone_until_two(on_checkpoint: CheckpointResponse) -> ActorSpawnZone {
    ActorSpawnZone {
        switch_inverted: false,
        carrier: CarrierId::WORLD,
        level: 0,
        levels: 1,
        roam_distance: 0.0,
        cols: [0, 1],
        rows: [0, 1],
        kind: test_kinds::CONTACT.into(),
        count: vec![1],
        respawn_secs: None,
        switch: None,
        until_checkpoint: Some(2),
        on_checkpoint,
    }
}

#[test]
fn leaving_a_carrier_keeps_an_actor_alive_until_it_falls_below_the_world() {
    let mut app = removal_app(None);
    let id = ActorId(1);
    let entity = spawn_actor(
        &mut app,
        id,
        Position {
            x: 100.0,
            y: -1.0,
            z: 0.0,
        },
    );
    app.update();
    assert!(app.world().resource::<ActorMap>().get(&id).is_some());
    app.world_mut()
        .get_mut::<Position>(entity)
        .expect("actor position missing")
        .y = CHARACTER_FALL_DEATH_Y - 1.0;
    app.update();
    assert!(app.world().resource::<ActorMap>().get(&id).is_none());
}

#[test]
fn a_destroying_zone_self_destructs_its_actors_once_the_course_reaches_its_checkpoint() {
    let mut app = removal_app(Some(zone_until_two(CheckpointResponse::Destroy)));
    let id = ActorId(1);
    let entity = spawn_actor(&mut app, id, Position::default());
    let rx = add_player_at(&mut app, PlayerId(1), Some(0));
    app.update();
    assert!(
        app.world().resource::<ActorMap>().get(&id).is_some(),
        "checkpoint 1 is short of the zone's checkpoint 2"
    );

    {
        let mut players = app.world_mut().resource_mut::<PlayerMap>();
        set_checkpoint(players.get_mut(&PlayerId(1)).expect("player missing"), Some(1));
    }
    app.update();
    assert!(app.world().resource::<ActorMap>().get(&id).is_none());
    assert!(app.world().get_entity(entity).is_err());
    let death = std::iter::from_fn(|| rx.try_recv().ok())
        .find_map(|message| match message {
            ServerMessage::ActorDeath(death) => Some(death),
            _ => None,
        })
        .expect("self-destruct sent no death");
    assert_eq!(death.id, id);
    assert_eq!(death.killer, None, "a self-destruct credits nobody");
    assert!(matches!(
        app.world().resource::<PendingExplosions>().0.front(),
        Some(PendingExplosion::Actor { source_id, .. }) if *source_id == id
    ));
}

#[test]
fn a_stopping_zone_keeps_its_actors_past_its_checkpoint() {
    let mut app = removal_app(Some(zone_until_two(CheckpointResponse::Stop)));
    let id = ActorId(1);
    spawn_actor(&mut app, id, Position::default());
    add_player_at(&mut app, PlayerId(1), Some(1));
    app.update();
    assert!(app.world().resource::<ActorMap>().get(&id).is_some());
    assert!(app.world().resource::<PendingExplosions>().0.is_empty());
}
