use bevy::prelude::*;
use crossbeam_channel::TryRecvError;

use common::network::{channel_for, encode_message};

use super::{
    resources::{ClientLinks, LinkSource},
    transport::Listener,
};

// Hands every remote player's queued replies to renet and puts the tick's
// packets on the wire; runs last so nothing sent this tick waits a tick.
pub(super) fn network_flush_system(mut links: ResMut<ClientLinks>, listener: Option<ResMut<Listener>>) {
    let Some(mut listener) = listener else {
        return;
    };
    for (id, source) in links.iter_mut() {
        let LinkSource::Remote { client_id, outgoing } = source else {
            continue;
        };
        loop {
            match outgoing.try_recv() {
                Ok(message) => match encode_message(&message) {
                    Ok(bytes) => {
                        let channel = channel_for(message.lane(), bytes.len());
                        listener.server.send_message(*client_id, channel, bytes);
                    }
                    Err(error) => error!("dropping a message to player#{}: {error}", id.0),
                },
                Err(TryRecvError::Empty) => break,
                // `hang_up` dropped the sender; netcode reports the disconnect
                // on a later poll, which removes the link.
                Err(TryRecvError::Disconnected) => {
                    listener.server.disconnect(*client_id);
                    break;
                }
            }
        }
    }
    listener.send_packets();
}
