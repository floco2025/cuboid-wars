use bevy_ecs::prelude::*;

// The tick a simulation step corresponds to on the server. The server counts
// it, one per update, and stamps it on the state messages. The client counts
// its own fixed steps and estimates current server time from pongs and RTT.
// World motion shared by both sides is a pure function of this tick.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ServerTick(pub u32);

pub fn server_tick_advance_system(mut tick: ResMut<ServerTick>) {
    tick.0 = tick.0.wrapping_add(1);
}

// A configured duration as whole ticks, so both sides time it from the
// shared tick alone.
#[must_use]
pub fn ticks_from_secs(secs: f32, server_hz: u32) -> u32 {
    (secs * server_hz as f32).round() as u32
}
