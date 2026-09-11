// UDP transport shared by client and server, built on renet: three channels,
// selected per message by the supplied `protocol::Lane` and the encoded
// size; `protocol.rs` owns the lane assignment.
//
// * Reliable ordered: `Lane::Reliable`. Lost data is sent again, which can
//   delay the messages behind it.
// * Unreliable: `Lane::Unreliable` messages that fit one packet. A lost
//   message is simply gone.
// * Retransmitted, unordered: `Lane::Unreliable` messages larger than one
//   packet. renet slices such a message across packets, and on the
//   unreliable channel one lost slice loses the whole message, so a
//   map-sized snapshot would rarely survive real loss. Sending it again until
//   it arrives keeps the rule the sequence guards were written for: a small
//   update may be lost, a large one arrives late. Which channel a snapshot
//   takes depends on the map.
//
// The transport never discards a message on its own. netcode encrypts each
// session with the unsecure zero key, so anyone may connect; the connect
// token a client generates expires after five minutes, so client and server
// wall clocks must agree to within that.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};
use bincode::{Decode, Encode};
use renet::{Bytes, ChannelConfig, ConnectionConfig, SendType};

use crate::protocol::Lane;

// Separates this game's packets from anything else on the port.
pub const PROTOCOL_ID: u64 = 0x6375_626f_6964_0001;
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
pub const MAX_CLIENTS: usize = 32;
// renet's slice size, which it does not export: a message above it spans packets.
pub const SLICE_BYTES: usize = 1200;
pub const RELIABLE_CHANNEL: u8 = 0;
pub const UNRELIABLE_CHANNEL: u8 = 1;
pub const RETRANSMITTED_CHANNEL: u8 = 2;
pub const CHANNELS: [u8; 3] = [RELIABLE_CHANNEL, UNRELIABLE_CHANNEL, RETRANSMITTED_CHANNEL];
// Sized so `SInit` and a burst of snapshots never trip a channel's cap, which
// disconnects a reliable channel.
const CHANNEL_MEMORY_BYTES: usize = 16 * 1024 * 1024;
// Bytes renet may put on the wire per poll, sized so a large snapshot leaves
// in the tick that produced it instead of being metered across ticks.
const BYTES_PER_POLL: u64 = 8 * 1024 * 1024;
const RELIABLE_RESEND: Duration = Duration::from_millis(100);

pub fn connection_config() -> ConnectionConfig {
    let channels = || {
        vec![
            ChannelConfig {
                channel_id: RELIABLE_CHANNEL,
                max_memory_usage_bytes: CHANNEL_MEMORY_BYTES,
                send_type: SendType::ReliableOrdered {
                    resend_time: RELIABLE_RESEND,
                },
            },
            ChannelConfig {
                channel_id: UNRELIABLE_CHANNEL,
                max_memory_usage_bytes: CHANNEL_MEMORY_BYTES,
                send_type: SendType::Unreliable,
            },
            ChannelConfig {
                channel_id: RETRANSMITTED_CHANNEL,
                max_memory_usage_bytes: CHANNEL_MEMORY_BYTES,
                send_type: SendType::ReliableUnordered {
                    resend_time: RELIABLE_RESEND,
                },
            },
        ]
    };
    ConnectionConfig {
        available_bytes_per_tick: BYTES_PER_POLL,
        server_channels_config: channels(),
        client_channels_config: channels(),
    }
}

// The channel a message rides, from its lane and encoded size.
#[must_use]
pub const fn channel_for(lane: Lane, len: usize) -> u8 {
    match lane {
        Lane::Reliable => RELIABLE_CHANNEL,
        Lane::Unreliable if len <= SLICE_BYTES => UNRELIABLE_CHANNEL,
        Lane::Unreliable => RETRANSMITTED_CHANNEL,
    }
}

pub fn encode_message<T: Encode>(message: &T) -> Result<Bytes> {
    let payload = bincode::encode_to_vec(message, bincode::config::standard())?;
    if payload.len() > MAX_MESSAGE_BYTES {
        bail!("message of {} bytes exceeds {MAX_MESSAGE_BYTES}", payload.len());
    }
    Ok(Bytes::from(payload))
}

pub fn decode_message<T: Decode<()>>(bytes: &[u8]) -> Result<T> {
    let (message, read) = bincode::decode_from_slice(bytes, bincode::config::standard())?;
    if read != bytes.len() {
        bail!("message has {} trailing bytes", bytes.len() - read);
    }
    Ok(message)
}

// netcode's clock: time since the Unix epoch.
#[must_use]
pub fn unix_now() -> Duration {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO)
}

#[cfg(test)]
#[path = "tests/network.rs"]
mod tests;
