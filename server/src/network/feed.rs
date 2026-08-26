use super::{ServerToClient, broadcast_to_all, broadcast_to_others};
use crate::{
    config::FeedConfig,
    players::{PlayerInfo, PlayerMap},
};
use common::protocol::{FeedEvent, PlayerId, SFeed, ServerMessage};

pub fn announce(players: &PlayerMap, feed: &FeedConfig, event: FeedEvent) {
    if feed.announces(&event) {
        broadcast_to_all(players, ServerMessage::Feed(SFeed { event }));
    }
}

pub fn announce_to_others(players: &PlayerMap, feed: &FeedConfig, skip: PlayerId, event: FeedEvent) {
    if feed.announces(&event) {
        broadcast_to_others(players, skip, ServerMessage::Feed(SFeed { event }));
    }
}

pub fn reply(info: &PlayerInfo, event: FeedEvent) {
    let _ = info
        .channel
        .send(ServerToClient::Send(ServerMessage::Feed(SFeed { event })));
}

#[cfg(test)]
mod tests {
    use bevy::prelude::Entity;
    use tokio::sync::mpsc::unbounded_channel;

    use super::*;

    #[test]
    fn announce_skips_disabled_events() {
        let (tx, mut rx) = unbounded_channel();
        let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
        info.logged_in = true;
        let mut players = PlayerMap::default();
        players.insert(PlayerId(1), info);
        let chat = || FeedEvent::Chat {
            name: "Marc".to_owned(),
            text: "hi".to_owned(),
        };

        let mut feed = FeedConfig::all(true, &[]);
        feed.chat = false;
        announce(&players, &feed, chat());
        assert!(rx.try_recv().is_err(), "disabled event must not be sent");

        feed.chat = true;
        announce(&players, &feed, chat());
        assert!(matches!(
            rx.try_recv().expect("enabled event is broadcast"),
            ServerToClient::Send(ServerMessage::Feed(SFeed { event })) if event == chat()
        ));
    }
}
