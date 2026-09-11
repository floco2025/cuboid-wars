use super::{broadcast_to_all, broadcast_to_others};
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
    Crushed,
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
    // A press that turned a switch on, credited to `name`.
    SwitchOn {
        name: String,
        switch_name: String,
    },
    SwitchOff {
        switch_name: String,
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
                let _ = info.connection.channel.send(message);
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
        FeedEvent::SwitchOn { .. } => config.switch_on,
        FeedEvent::SwitchOff { .. } => config.switch_off,
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
            span("key", FeedStyle::Key(kind)),
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
        FeedEvent::SwitchOn { name, switch_name } => {
            one(format!("{name} turned on the {switch_name} switch"), FeedStyle::Default)
        }
        FeedEvent::SwitchOff { switch_name } => one(format!("The {switch_name} switch turned off"), FeedStyle::Dim),
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
        DeathCause::Crushed => one(format!("{name} was crushed"), FeedStyle::Dim),
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
#[path = "tests/feed.rs"]
mod tests;
