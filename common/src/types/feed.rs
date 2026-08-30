use bincode::{Decode, Encode};

use super::barrier_kind::BarrierKindId;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode)]
pub enum FeedStyle {
    Default,
    Dim,
    Chat,
    Console,
    Barrier(BarrierKindId),
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct FeedSpan {
    pub text: String,
    pub style: FeedStyle,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{SFeed, ServerMessage};

    #[test]
    fn styled_feed_line_round_trips() {
        let message = ServerMessage::Feed(SFeed {
            spans: vec![
                FeedSpan {
                    text: "opened the ".to_owned(),
                    style: FeedStyle::Default,
                },
                FeedSpan {
                    text: "treasure".to_owned(),
                    style: FeedStyle::Barrier(BarrierKindId(2)),
                },
            ],
        });
        let bytes = bincode::encode_to_vec(&message, bincode::config::standard()).expect("feed line failed to encode");
        let (decoded, _): (ServerMessage, usize) =
            bincode::decode_from_slice(&bytes, bincode::config::standard()).expect("feed line failed to decode");

        let ServerMessage::Feed(expected) = message else {
            unreachable!();
        };
        let ServerMessage::Feed(actual) = decoded else {
            panic!("decoded a different message");
        };
        assert_eq!(actual, expected);
    }
}
