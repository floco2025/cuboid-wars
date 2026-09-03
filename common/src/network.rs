// QUIC transport shared by client and server: two lanes, selected per message
// by `protocol::Lane`.
//
// * Reliable: one bidirectional stream per connection, opened by the client
//   right after connecting and accepted by the server before it reads
//   anything. Frames are a little-endian u32 payload length followed by the
//   bincode payload, capped at `MAX_MESSAGE_BYTES`.
// * Unreliable: the bincode payload alone, as a QUIC datagram when it fits
//   one packet, otherwise on its own unidirectional stream written and
//   finished in one go. The lane never drops anything and keeps no state. The
//   two carriers differ under loss: a datagram is simply gone, while a stream
//   message is retransmitted and can hold up the next stream behind it.
//   Snapshots cross the datagram limit as the world fills, so which carrier
//   they take depends on the map.

use anyhow::{Context, Result, bail};
use bevy::log::{error, warn};
use bincode::{Decode, Encode};
use bytes::Bytes;
use quinn::{Connection, ReadExactError, ReadToEndError, RecvStream, SendDatagramError, SendStream};

use crate::protocol::Lane;

pub const ALPN_PROTOCOL: &[u8] = b"cuboid-wars/1";
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const FRAME_HEADER_BYTES: usize = 4;

pub fn encode_message<T: Encode>(message: &T) -> Result<Vec<u8>> {
    let payload = bincode::encode_to_vec(message, bincode::config::standard())?;
    if payload.len() > MAX_MESSAGE_BYTES {
        bail!("message of {} bytes exceeds {MAX_MESSAGE_BYTES}", payload.len());
    }
    Ok(payload)
}

fn decode_message<T: Decode<()>>(bytes: &[u8]) -> Result<T> {
    let (message, read) = bincode::decode_from_slice(bytes, bincode::config::standard())?;
    if read != bytes.len() {
        bail!("message has {} trailing bytes", bytes.len() - read);
    }
    Ok(message)
}

fn encode_frame<T: Encode>(message: &T) -> Result<Vec<u8>> {
    let payload = encode_message(message)?;
    let len = u32::try_from(payload.len()).context("frame payload length does not fit u32")?;
    let mut frame = Vec::with_capacity(FRAME_HEADER_BYTES + payload.len());
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

fn frame_payload_len(header: [u8; FRAME_HEADER_BYTES]) -> Result<usize> {
    let len = usize::try_from(u32::from_le_bytes(header)).context("frame length does not fit usize")?;
    if len > MAX_MESSAGE_BYTES {
        bail!("frame payload of {len} bytes exceeds {MAX_MESSAGE_BYTES}");
    }
    Ok(len)
}

pub async fn write_framed<T: Encode>(stream: &mut SendStream, message: &T) -> Result<()> {
    stream.write_all(&encode_frame(message)?).await?;
    Ok(())
}

async fn write_own_stream(connection: &Connection, bytes: &[u8]) -> Result<()> {
    let mut stream = connection.open_uni().await?;
    stream.write_all(bytes).await?;
    stream.finish()?;
    Ok(())
}

// Datagram when it fits the path, own stream otherwise; a `TooLarge` means
// the path limit shrank since the check, so the stream takes it after all.
async fn send_unreliable<T: Encode>(connection: &Connection, message: &T) -> Result<()> {
    let payload = Bytes::from(encode_message(message)?);
    if connection.max_datagram_size().is_some_and(|max| payload.len() <= max) {
        match connection.send_datagram(payload.clone()) {
            Ok(()) => return Ok(()),
            Err(SendDatagramError::TooLarge) => {}
            Err(error) => return Err(error.into()),
        }
    }
    write_own_stream(connection, &payload).await
}

pub async fn send_message<T: Encode>(
    connection: &Connection,
    reliable: &mut SendStream,
    lane: Lane,
    message: &T,
) -> Result<()> {
    match lane {
        Lane::Reliable => write_framed(reliable, message).await,
        Lane::Unreliable => send_unreliable(connection, message).await,
    }
}

// Runs the reliable and both unreliable receive loops until the connection
// ends. They are joined, never selected over: a framed read dropped mid-frame
// would desynchronise the reliable stream for good. `drop` is the client's
// `--drop` simulation: a hit discards the raw bytes before they are read.
pub async fn receive_lanes<T: Decode<()>>(
    connection: &Connection,
    reliable: &mut RecvStream,
    forward: impl FnMut(T) -> Result<()> + Copy,
    drop: impl FnMut() -> bool + Copy,
) {
    tokio::join!(
        drive_lane(connection, "reliable", receive_reliable(reliable, forward)),
        drive_lane(
            connection,
            "unreliable datagrams",
            receive_unreliable_datagrams(connection, forward, drop)
        ),
        drive_lane(
            connection,
            "unreliable streams",
            receive_unreliable_streams(connection, forward, drop)
        ),
    );
}

// A lane that ends takes the connection down with it, so the others unwind.
pub async fn drive_lane(connection: &Connection, lane: &str, run: impl Future<Output = Result<()>>) {
    let outcome = run.await;
    if connection.close_reason().is_some() {
        return;
    }
    match outcome {
        Ok(()) => connection.close(0u32.into(), b"lane ended"),
        Err(error) => {
            error!("{lane} lane failed: {error:#}");
            connection.close(1u32.into(), b"protocol error");
        }
    }
}

// Ends cleanly when the peer finishes its stream between frames.
async fn receive_reliable<T: Decode<()>>(
    stream: &mut RecvStream,
    mut forward: impl FnMut(T) -> Result<()>,
) -> Result<()> {
    loop {
        let mut header = [0u8; FRAME_HEADER_BYTES];
        match stream.read_exact(&mut header).await {
            Ok(()) => {}
            Err(ReadExactError::FinishedEarly(0)) => return Ok(()),
            Err(error) => return Err(error.into()),
        }
        let mut payload = vec![0u8; frame_payload_len(header)?];
        stream.read_exact(&mut payload).await?;
        forward(decode_message(&payload)?)?;
    }
}

async fn receive_unreliable_datagrams<T: Decode<()>>(
    connection: &Connection,
    mut forward: impl FnMut(T) -> Result<()>,
    mut drop: impl FnMut() -> bool,
) -> Result<()> {
    loop {
        let bytes = connection.read_datagram().await?;
        forward_unreliable("datagram", &bytes, &mut forward, &mut drop)?;
    }
}

async fn receive_unreliable_streams<T: Decode<()>>(
    connection: &Connection,
    mut forward: impl FnMut(T) -> Result<()>,
    mut drop: impl FnMut() -> bool,
) -> Result<()> {
    loop {
        let mut stream = connection.accept_uni().await?;
        let bytes = match stream.read_to_end(MAX_MESSAGE_BYTES).await {
            Ok(bytes) => bytes,
            Err(ReadToEndError::TooLong) => {
                warn!("skipping an unreliable stream message over {MAX_MESSAGE_BYTES} bytes");
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        forward_unreliable("stream", &bytes, &mut forward, &mut drop)?;
    }
}

// A message the unreliable lane cannot decode is a bug on the sending side
// and is skipped, not fatal; only a dead `forward` ends the loop.
fn forward_unreliable<T: Decode<()>>(
    carrier: &str,
    bytes: &[u8],
    forward: &mut impl FnMut(T) -> Result<()>,
    drop: &mut impl FnMut() -> bool,
) -> Result<()> {
    if drop() {
        return Ok(());
    }
    match decode_message(bytes) {
        Ok(message) => forward(message),
        Err(error) => {
            warn!("skipping an undecodable unreliable {carrier} message: {error}");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{CLogin, ClientMessage};

    fn login() -> ClientMessage {
        ClientMessage::Login(CLogin {
            name: "Marc".to_owned(),
        })
    }

    #[test]
    fn frame_round_trips_message() {
        let frame = encode_frame(&login()).expect("login failed to encode");
        let (header, payload) = frame
            .split_first_chunk::<FRAME_HEADER_BYTES>()
            .expect("frame has no header");
        assert_eq!(frame_payload_len(*header).expect("bad frame length"), payload.len());
        let ClientMessage::Login(login) = decode_message::<ClientMessage>(payload).expect("frame failed to decode")
        else {
            panic!("decoded a different variant");
        };
        assert_eq!(login.name, "Marc");
    }

    #[test]
    fn frame_rejects_length_over_cap() {
        let len = u32::try_from(MAX_MESSAGE_BYTES + 1).expect("cap plus one does not fit u32");
        let error = frame_payload_len(len.to_le_bytes()).expect_err("oversize frame accepted");
        assert!(error.to_string().contains("exceeds"));
    }

    #[test]
    fn decode_rejects_trailing_bytes() {
        let mut bytes = encode_message(&login()).expect("login failed to encode");
        bytes.push(0);
        assert!(decode_message::<ClientMessage>(&bytes).is_err());
    }
}
