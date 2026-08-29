use bincode::{Decode, Encode};

use super::barrier_kind::BarrierKindId;

// How a player died, as the feed tells it. Self-kills are explicit variants:
// names aren't unique, so the client can't infer one by comparing `by` to
// the victim.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum DeathCause {
    Shot { by: String },
    SelfShot,
    Missile { by: String },
    SelfMissile,
    Beam { kind: String },
    PlayerBlast { by: String },
    ActorBlast { kind: String },
    Fall,
    Admin,
}

// One line of the game message feed. Server-authored: names and kinds are
// resolved at emit time so the client renders without live entity maps.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum FeedEvent {
    PlayerJoined {
        name: String,
    },
    PlayerLeft {
        name: String,
    },
    PlayerDied {
        name: String,
        cause: DeathCause,
    },
    ActorDestroyed {
        name: String,
        kind: String,
    },
    KeyFound {
        name: String,
        kind: BarrierKindId,
    },
    QuestCompleted {
        name: String,
        title: String,
    },
    // A player finished their part of an `everyone` quest.
    QuestPartDone {
        name: String,
        title: String,
        players_done: u32,
        players: u32,
    },
    GroupQuestCompleted {
        title: String,
    },
    BarrierOpened {
        name: String,
        kind: BarrierKindId,
    },
    BarrierClosed {
        kind: BarrierKindId,
    },
    // Unicast to the issuer of a `CAdmin` command.
    AdminReply {
        text: String,
    },
    // Broadcast: a world-affecting admin command and who issued it.
    AdminAction {
        name: String,
        text: String,
    },
    Chat {
        name: String,
        text: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{SFeed, ServerMessage};

    fn round_trip(event: FeedEvent) {
        let message = ServerMessage::Feed(SFeed { event: event.clone() });
        let bytes = bincode::encode_to_vec(&message, bincode::config::standard()).expect("feed event failed to encode");
        let (decoded, _): (ServerMessage, usize) =
            bincode::decode_from_slice(&bytes, bincode::config::standard()).expect("feed event failed to decode");
        match decoded {
            ServerMessage::Feed(SFeed { event: decoded }) => assert_eq!(decoded, event),
            other => panic!("decoded a different message: {other:?}"),
        }
    }

    #[test]
    fn feed_events_round_trip_through_bincode() {
        let name = || "Marc".to_owned();
        let other = || "Bob".to_owned();
        let kind = BarrierKindId(2);
        let deaths = [
            DeathCause::Shot { by: other() },
            DeathCause::SelfShot,
            DeathCause::Missile { by: other() },
            DeathCause::SelfMissile,
            DeathCause::Beam {
                kind: "zapper".to_owned(),
            },
            DeathCause::PlayerBlast { by: other() },
            DeathCause::ActorBlast {
                kind: "mine".to_owned(),
            },
            DeathCause::Fall,
            DeathCause::Admin,
        ];
        for cause in deaths {
            round_trip(FeedEvent::PlayerDied { name: name(), cause });
        }
        for event in [
            FeedEvent::PlayerJoined { name: name() },
            FeedEvent::PlayerLeft { name: name() },
            FeedEvent::ActorDestroyed {
                name: name(),
                kind: "reaper".to_owned(),
            },
            FeedEvent::KeyFound { name: name(), kind },
            FeedEvent::QuestCompleted {
                name: name(),
                title: "Gold Rush".to_owned(),
            },
            FeedEvent::QuestPartDone {
                name: name(),
                title: "Gold Rush".to_owned(),
                players_done: 2,
                players: 3,
            },
            FeedEvent::GroupQuestCompleted {
                title: "Gold Rush".to_owned(),
            },
            FeedEvent::BarrierOpened { name: name(), kind },
            FeedEvent::BarrierClosed { kind },
            FeedEvent::AdminReply {
                text: "/help\n/kick".to_owned(),
            },
            FeedEvent::AdminAction {
                name: name(),
                text: "weather set to rain".to_owned(),
            },
            FeedEvent::Chat {
                name: name(),
                text: "hi".to_owned(),
            },
        ] {
            round_trip(event);
        }
    }
}
