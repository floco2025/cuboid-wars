use super::{ServerToClient, broadcast_to_all, broadcast_to_others};
use crate::{config::FeedConfig, players::PlayerMap};
use common::protocol::{BarrierKindId, FeedSpan, FeedStyle, PlayerId, SFeed, ServerMessage};

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FeedAudience {
    Everyone,
    EveryoneExcept(PlayerId),
    Player(PlayerId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    EveryoneQuestPartDone {
        name: String,
        title: String,
        players_done: u32,
        players_total: u32,
    },
    GroupQuestCompleted {
        title: String,
    },
    BarrierOpened {
        name: String,
        kind: BarrierKindId,
        kind_name: String,
    },
    BarrierClosed {
        kind: BarrierKindId,
        kind_name: String,
    },
    AdminReply {
        text: String,
    },
    AdminAction {
        name: String,
        text: String,
    },
    Chat {
        name: String,
        text: String,
    },
}

pub fn emit_feed(players: &PlayerMap, config: &FeedConfig, audience: FeedAudience, event: FeedEvent) {
    if !matches!(audience, FeedAudience::Player(_)) && !announces(config, &event) {
        return;
    }
    let message = ServerMessage::Feed(render(event));
    match audience {
        FeedAudience::Everyone => broadcast_to_all(players, message),
        FeedAudience::EveryoneExcept(skip) => broadcast_to_others(players, skip, message),
        FeedAudience::Player(player) => {
            if let Some(info) = players.get(&player) {
                let _ = info.channel.send(ServerToClient::Send(message));
            }
        }
    }
}

fn announces(config: &FeedConfig, event: &FeedEvent) -> bool {
    match event {
        FeedEvent::PlayerJoined { .. } => config.player_joined,
        FeedEvent::PlayerLeft { .. } => config.player_left,
        FeedEvent::PlayerDied { .. } => config.player_died,
        FeedEvent::ActorDestroyed { kind, .. } => config
            .actor_destroyed
            .get(kind)
            .copied()
            .expect("actor kind missing from feed.actor_destroyed"),
        FeedEvent::KeyFound { .. } => config.key_found,
        FeedEvent::QuestCompleted { .. } => config.quest_completed,
        FeedEvent::EveryoneQuestPartDone { .. } => config.quest_part_done,
        FeedEvent::GroupQuestCompleted { .. } => config.group_quest_completed,
        FeedEvent::BarrierOpened { .. } => config.barrier_opened,
        FeedEvent::BarrierClosed { .. } => config.barrier_closed,
        FeedEvent::AdminReply { .. } => false,
        FeedEvent::AdminAction { .. } => config.admin_action,
        FeedEvent::Chat { .. } => config.chat,
    }
}

fn render(event: FeedEvent) -> SFeed {
    let spans = match event {
        FeedEvent::PlayerJoined { name } => one(format!("{name} joined"), FeedStyle::Dim),
        FeedEvent::PlayerLeft { name } => one(format!("{name} left"), FeedStyle::Dim),
        FeedEvent::PlayerDied { name, cause } => render_death(name, cause),
        FeedEvent::ActorDestroyed { name, kind } => one(format!("{name} destroyed a {kind}"), FeedStyle::Default),
        FeedEvent::KeyFound { name, kind } => vec![
            span(format!("{name} found a "), FeedStyle::Default),
            span("key", FeedStyle::Barrier(kind)),
        ],
        FeedEvent::QuestCompleted { name, title } => one(format!("{name} completed {title}"), FeedStyle::Default),
        FeedEvent::EveryoneQuestPartDone {
            name,
            title,
            players_done,
            players_total,
        } => one(
            format!("{name} finished {title} ({players_done}/{players_total} players)"),
            FeedStyle::Default,
        ),
        FeedEvent::GroupQuestCompleted { title } => one(format!("Everyone completed {title}"), FeedStyle::Default),
        FeedEvent::BarrierOpened { name, kind, kind_name } => vec![
            span(format!("{name} opened the "), FeedStyle::Default),
            span(kind_name, FeedStyle::Barrier(kind)),
            span(" barriers", FeedStyle::Default),
        ],
        FeedEvent::BarrierClosed { kind, kind_name } => vec![
            span("The ", FeedStyle::Dim),
            span(kind_name, FeedStyle::Barrier(kind)),
            span(" barriers closed", FeedStyle::Dim),
        ],
        FeedEvent::AdminReply { text } => one(text, FeedStyle::Console),
        FeedEvent::AdminAction { name, text } => one(format!("{name}: {text}"), FeedStyle::Console),
        FeedEvent::Chat { name, text } => one(format!("{name}: {text}"), FeedStyle::Chat),
    };
    SFeed { spans }
}

fn render_death(name: String, cause: DeathCause) -> Vec<FeedSpan> {
    match cause {
        DeathCause::Shot { by } => one(format!("{by} shot {name}"), FeedStyle::Default),
        DeathCause::SelfShot => one(format!("{name} shot themselves"), FeedStyle::Default),
        DeathCause::Missile { by } => one(format!("{by} blew up {name}"), FeedStyle::Default),
        DeathCause::SelfMissile => one(format!("{name} blew themselves up"), FeedStyle::Default),
        DeathCause::Beam { kind } => one(format!("{name} was zapped by a {kind}"), FeedStyle::Default),
        DeathCause::ActorBlast { kind } => one(format!("{name} was blown up by a {kind}"), FeedStyle::Default),
        DeathCause::PlayerBlast { by } => one(format!("{name} was caught in {by}'s explosion"), FeedStyle::Default),
        DeathCause::Fall => one(format!("{name} fell"), FeedStyle::Dim),
        DeathCause::Admin => one(format!("{name} was killed by an admin"), FeedStyle::Default),
    }
}

fn one(text: String, style: FeedStyle) -> Vec<FeedSpan> {
    vec![FeedSpan { text, style }]
}

fn span(text: impl Into<String>, style: FeedStyle) -> FeedSpan {
    FeedSpan {
        text: text.into(),
        style,
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::Entity;
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

    use super::*;
    use crate::players::PlayerInfo;

    fn players() -> (PlayerMap, UnboundedReceiver<ServerToClient>) {
        let (tx, rx) = unbounded_channel();
        let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
        info.logged_in = true;
        let mut players = PlayerMap::default();
        players.insert(PlayerId(1), info);
        (players, rx)
    }

    fn chat() -> FeedEvent {
        FeedEvent::Chat {
            name: "Marc".to_owned(),
            text: "hi".to_owned(),
        }
    }

    fn receive(rx: &mut UnboundedReceiver<ServerToClient>) -> SFeed {
        match rx.try_recv().expect("feed line missing") {
            ServerToClient::Send(ServerMessage::Feed(line)) => line,
            other => panic!("unexpected envelope: {other:?}"),
        }
    }

    fn text(line: &SFeed) -> String {
        line.spans.iter().map(|span| span.text.as_str()).collect()
    }

    #[test]
    fn public_delivery_obeys_the_event_switch() {
        let (players, mut rx) = players();
        let mut config = FeedConfig::all(true, &[]);
        config.chat = false;

        emit_feed(&players, &config, FeedAudience::Everyone, chat());
        assert!(rx.try_recv().is_err());

        config.chat = true;
        emit_feed(&players, &config, FeedAudience::Everyone, chat());
        assert_eq!(text(&receive(&mut rx)), "Marc: hi");
    }

    #[test]
    fn private_delivery_bypasses_public_switches() {
        let (players, mut rx) = players();
        let config = FeedConfig::all(false, &[]);

        emit_feed(
            &players,
            &config,
            FeedAudience::Player(PlayerId(1)),
            FeedEvent::AdminReply {
                text: "not authorized".to_owned(),
            },
        );

        assert_eq!(text(&receive(&mut rx)), "not authorized");
    }

    #[test]
    fn everyone_except_skips_only_the_named_player() {
        let (mut players, mut first) = players();
        let (tx, mut second) = unbounded_channel();
        let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
        info.logged_in = true;
        players.insert(PlayerId(2), info);

        emit_feed(
            &players,
            &FeedConfig::all(true, &[]),
            FeedAudience::EveryoneExcept(PlayerId(1)),
            chat(),
        );

        assert!(first.try_recv().is_err());
        assert_eq!(text(&receive(&mut second)), "Marc: hi");
    }

    #[test]
    fn actor_switch_is_selected_by_kind() {
        let mut config = FeedConfig::all(false, &["mine", "sentry"]);
        config.actor_destroyed.insert("sentry".to_owned(), true);

        assert!(announces(
            &config,
            &FeedEvent::ActorDestroyed {
                name: "Marc".to_owned(),
                kind: "sentry".to_owned(),
            }
        ));
        assert!(!announces(
            &config,
            &FeedEvent::ActorDestroyed {
                name: "Marc".to_owned(),
                kind: "mine".to_owned(),
            }
        ));
    }

    #[test]
    fn death_wording_is_resolved_before_the_wire() {
        let line = render(FeedEvent::PlayerDied {
            name: "Marc".to_owned(),
            cause: DeathCause::Shot { by: "Bob".to_owned() },
        });

        assert_eq!(text(&line), "Bob shot Marc");
        assert_eq!(line.spans[0].style, FeedStyle::Default);
    }

    #[test]
    fn barrier_name_and_style_are_explicit() {
        let line = render(FeedEvent::BarrierOpened {
            name: "Marc".to_owned(),
            kind: BarrierKindId(2),
            kind_name: "treasure".to_owned(),
        });

        assert_eq!(text(&line), "Marc opened the treasure barriers");
        assert_eq!(line.spans[1].style, FeedStyle::Barrier(BarrierKindId(2)));
    }
}
