use bevy_ecs::prelude::*;

// The tick a simulation step corresponds to on the server. The server counts
// it, one per update, and stamps it on the state messages. The client counts
// its own fixed steps and keeps the value at the server tick that will apply
// what it commits now — server time plus one-way latency — corrected from the
// server's echoes of its own commits (`TickSync` in the client). World motion
// that both sides must agree on at tick granularity is a pure function of it.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ServerTick(pub u32);

pub fn server_tick_advance_system(mut tick: ResMut<ServerTick>) {
    tick.0 = tick.0.wrapping_add(1);
}
