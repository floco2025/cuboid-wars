use super::*;
use crate::protocol::{CLogin, ClientMessage};

#[test]
fn decode_takes_exactly_one_message() {
    let login = ClientMessage::Login(CLogin {
        name: "Alex".to_owned(),
    });
    let mut bytes = encode_message(&login).expect("login failed to encode").to_vec();
    let ClientMessage::Login(decoded) = decode_message::<ClientMessage>(&bytes).expect("login failed to decode") else {
        panic!("decoded a different variant");
    };
    assert_eq!(decoded.name, "Alex");
    bytes.push(0);
    assert!(decode_message::<ClientMessage>(&bytes).is_err());
}

#[test]
fn unreliable_messages_change_channel_at_the_slice_size() {
    assert_eq!(channel_for(Lane::Reliable, 0), RELIABLE_CHANNEL);
    assert_eq!(channel_for(Lane::Reliable, MAX_MESSAGE_BYTES), RELIABLE_CHANNEL);
    assert_eq!(channel_for(Lane::Unreliable, SLICE_BYTES), UNRELIABLE_CHANNEL);
    assert_eq!(channel_for(Lane::Unreliable, SLICE_BYTES + 1), RETRANSMITTED_CHANNEL);
}
