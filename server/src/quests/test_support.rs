use bevy::prelude::Entity;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::{QuestBoard, QuestCatalog, assign_quests};
use crate::{
    config::{Quest, QuestKind, ServerGameplayConfig},
    network::ServerToClient,
    players::{PlayerInfo, PlayerMap},
};
use common::protocol::{
    PlayerId, QuestId, QuestScope, QuestState, QuestStateProgress, QuestUpdateReason, ServerMessage,
};

pub(crate) fn quest(id: &str, kind: QuestKind, scope: QuestScope, threshold: u32, requires: Option<&str>) -> Quest {
    Quest {
        id: QuestId(id.to_owned()),
        kind,
        scope,
        requires: requires.map(|required| QuestId(required.to_owned())),
        actor_kind: None,
        threshold,
        points: 100,
        title: id.to_owned(),
        description: format!("do {id}"),
        completed_text: format!("{id} done"),
    }
}

// The shipped config with a synthetic catalog.
pub(crate) fn catalog(quests: Vec<Quest>) -> ServerGameplayConfig {
    let mut config = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
    config
        .maps
        .get_mut(&config.default_map)
        .expect("default map missing from server gameplay config")
        .quests = quests;
    config
}

// An active player with every unlocked quest assigned; the assignment batch
// is discarded so the receiver only sees what the test triggers.
pub(crate) fn join(
    players: &mut PlayerMap,
    id: u32,
    catalog: &QuestCatalog,
    board: &QuestBoard,
) -> UnboundedReceiver<ServerToClient> {
    join_with(players, id, catalog, board, false)
}

pub(crate) fn join_with(
    players: &mut PlayerMap,
    id: u32,
    catalog: &QuestCatalog,
    board: &QuestBoard,
    dead: bool,
) -> UnboundedReceiver<ServerToClient> {
    let (tx, mut rx) = unbounded_channel();
    let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
    info.connection.phase = crate::players::ConnectionPhase::Active;
    info.connection.name = format!("P{id}");
    if dead {
        info.begin_respawn(2.0);
    }
    let player = PlayerId(id);
    players.insert(player, info);
    assign_quests(players, player, catalog, board);
    while rx.try_recv().is_ok() {}
    rx
}

// What a fresh player would be assigned right now.
pub(crate) fn assignment_for(catalog: &QuestCatalog, board: &QuestBoard) -> Vec<QuestState> {
    let (tx, mut rx) = unbounded_channel();
    let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
    info.connection.phase = crate::players::ConnectionPhase::Active;
    let player = PlayerId(1);
    let mut players = PlayerMap::default();
    players.insert(player, info);
    assign_quests(&mut players, player, catalog, board);
    match rx.try_recv() {
        Ok(ServerToClient::Send(ServerMessage::QuestUpdates(message))) => message
            .updates
            .into_iter()
            .filter_map(|update| (update.reason == QuestUpdateReason::Assigned).then_some(update.quest))
            .collect(),
        _ => Vec::new(),
    }
}

pub(crate) fn drain(receiver: &mut UnboundedReceiver<ServerToClient>) -> Vec<ServerMessage> {
    let mut messages = Vec::new();
    while let Ok(envelope) = receiver.try_recv() {
        if let ServerToClient::Send(msg) = envelope {
            messages.push(msg);
        }
    }
    messages
}

pub(crate) fn completed(messages: &[ServerMessage], id: &str) -> bool {
    messages.iter().any(|msg| match msg {
        ServerMessage::QuestUpdates(message) => message
            .updates
            .iter()
            .any(|update| update.reason == QuestUpdateReason::Completed && update.quest.id.0 == id),
        _ => false,
    })
}

pub(crate) fn progress_values(messages: &[ServerMessage], id: &str) -> Vec<u32> {
    messages
        .iter()
        .flat_map(|msg| match msg {
            ServerMessage::QuestUpdates(message) => message.updates.as_slice(),
            _ => &[],
        })
        .filter_map(|update| {
            if update.reason != QuestUpdateReason::Progressed || update.quest.id.0 != id {
                return None;
            }
            Some(match update.quest.status.progress {
                QuestStateProgress::Individual { progress }
                | QuestStateProgress::Shared { progress }
                | QuestStateProgress::Everyone { progress, .. } => progress,
            })
        })
        .collect()
}

pub(crate) fn feed_lines(messages: &[ServerMessage]) -> Vec<String> {
    messages
        .iter()
        .filter_map(|msg| match msg {
            ServerMessage::Feed(line) => Some(line.spans.iter().map(|span| span.text.as_str()).collect()),
            _ => None,
        })
        .collect()
}

pub(crate) fn assigned_ids(messages: &[ServerMessage]) -> Vec<String> {
    messages
        .iter()
        .flat_map(|msg| match msg {
            ServerMessage::QuestUpdates(message) => message.updates.as_slice(),
            _ => &[],
        })
        .filter(|update| update.reason == QuestUpdateReason::Assigned)
        .map(|update| update.quest.id.0.clone())
        .collect()
}

pub(crate) fn score(players: &PlayerMap, id: u32) -> i32 {
    players.get(&PlayerId(id)).expect("player tracked").session.score
}

pub(crate) fn own_progress(players: &PlayerMap, id: u32, quest: &str) -> u32 {
    players.get(&PlayerId(id)).expect("player tracked").session.quest_states[&QuestId(quest.to_owned())]
        .own_progress()
        .expect("quest has own progress")
}
