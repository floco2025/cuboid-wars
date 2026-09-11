use std::time::Instant;

use bevy::prelude::*;

use super::resources::ServerLink;

// Runs last so every send of the frame, fixed steps included, leaves now.
// On the frame the app exits it also tells the server, which otherwise
// waits out the connection timeout.
pub(crate) fn network_flush_system(mut link: ResMut<ServerLink>, mut exit: MessageReader<AppExit>) {
    link.flush(Instant::now());
    if exit.read().next().is_some() {
        link.disconnect();
    }
}
