use super::*;
use crate::{
    actors::actor_visuals_plugin, cameras::camera_plugin, characters::character_sync_plugin, input::input_plugin,
    network::network_plugin,
};

#[test]
fn input_network_camera_and_weapon_ordering_has_no_cycles() {
    let mut app = App::new();
    configure_client_sets(&mut app);
    input_plugin(&mut app);
    network_plugin(&mut app);
    camera_plugin(&mut app);
    character_sync_plugin(&mut app);
    actor_visuals_plugin(&mut app);
    app.world_mut().schedule_scope(Update, |world, schedule| {
        schedule
            .initialize(world)
            .expect("input/network schedule contains conflicting ordering");
    });
}
