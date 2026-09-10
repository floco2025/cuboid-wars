use super::*;
use crate::protocol::{CLogin, ClientMessage};

fn login() -> ClientMessage {
    ClientMessage::Login(CLogin {
        name: "Alex".to_owned(),
    })
}

#[test]
fn frame_round_trips_message() {
    let frame = encode_frame(&login()).expect("login failed to encode");
    let (header, payload) = frame
        .split_first_chunk::<FRAME_HEADER_BYTES>()
        .expect("frame has no header");
    assert_eq!(frame_payload_len(*header).expect("bad frame length"), payload.len());
    let ClientMessage::Login(login) = decode_message::<ClientMessage>(payload).expect("frame failed to decode") else {
        panic!("decoded a different variant");
    };
    assert_eq!(login.name, "Alex");
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
