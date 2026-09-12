use super::*;
use crate::actors::{ActorInfo, test_kinds};
use common::protocol::CarrierId;

#[test]
fn leaving_a_carrier_keeps_an_actor_alive_until_it_falls_below_the_world() {
    let mut app = App::new();
    app.init_resource::<ActorMap>()
        .init_resource::<PlayerMap>()
        .init_resource::<PendingExplosions>()
        .insert_resource(test_kinds::server_config())
        .add_systems(Update, actors_removal_system);
    let id = ActorId(1);
    let entity = app
        .world_mut()
        .spawn((
            id,
            ActorMarker,
            Position {
                x: 100.0,
                y: -1.0,
                z: 0.0,
            },
            Health(100.0),
            ActorCrushed::default(),
        ))
        .id();
    app.world_mut()
        .resource_mut::<ActorMap>()
        .insert(id, ActorInfo::new(entity, 0, test_kinds::CONTACT.into(), CarrierId(1)));
    app.update();
    assert!(app.world().resource::<ActorMap>().get(&id).is_some());
    app.world_mut()
        .get_mut::<Position>(entity)
        .expect("actor position missing")
        .y = CHARACTER_FALL_DEATH_Y - 1.0;
    app.update();
    assert!(app.world().resource::<ActorMap>().get(&id).is_none());
}
