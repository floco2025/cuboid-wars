use super::*;
use crate::protocol::{CChat, CLogin, ClientMessage};

fn login() -> ClientMessage {
    ClientMessage::Login(CLogin {
        name: "Alex".to_owned(),
    })
}

#[test]
fn message_round_trips() {
    let bytes = encode_message(&login()).expect("login failed to encode");
    let ClientMessage::Login(login) = decode_message::<ClientMessage>(&bytes).expect("login failed to decode") else {
        panic!("decoded a different variant");
    };
    assert_eq!(login.name, "Alex");
}

#[test]
fn encode_rejects_messages_over_cap() {
    let chat = ClientMessage::Chat(CChat {
        text: "x".repeat(MAX_MESSAGE_BYTES + 1),
    });
    let error = encode_message(&chat).expect_err("oversize message accepted");
    assert!(error.to_string().contains("exceeds"));
}

#[test]
fn decode_rejects_trailing_bytes() {
    let mut bytes = encode_message(&login()).expect("login failed to encode").to_vec();
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

#[test]
fn both_directions_configure_every_channel() {
    let config = connection_config();
    for channels in [&config.server_channels_config, &config.client_channels_config] {
        let ids: Vec<u8> = channels.iter().map(|channel| channel.channel_id).collect();
        assert_eq!(ids, CHANNELS);
    }
}
