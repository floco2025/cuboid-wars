use std::{ops::ControlFlow, time::Instant};

use bevy::prelude::*;

use super::{context::ServerMessageContext, link::ServerLink, routing::route_server_message};

// Routes everything the link has received; a closed link ends the app.
pub(super) fn network_receive_system(
    mut commands: Commands,
    mut link: ResMut<ServerLink>,
    mut exit: MessageWriter<AppExit>,
    mut context: ServerMessageContext,
) {
    let received = link.receive(Instant::now(), |message| {
        route_server_message(message, &mut commands, &mut context);
        ControlFlow::Continue(())
    });
    if let Err(error) = received {
        error!("disconnected from server: {error:#}");
        exit.write(AppExit::Success);
    }
}
