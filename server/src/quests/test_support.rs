use bevy::prelude::Entity;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::{QuestBoard, QuestCatalog, assign_quests};
use crate::{
    config::{Quest, QuestKind, ServerGameplayConfig},
    network::ServerToClient,
    players::{PlayerInfo, PlayerMap},
};
use common::protocol::{NewQuest, PlayerId, QuestId, QuestScope, ServerMessage};

pub(crate) fn quest(id: &str, kind: QuestKind, scope: QuestScope, threshold: u32, requires: Option<&str>) -> Quest {
    Quest {
        id: QuestId(id.to_owned()),
        kind,
        scope,
        requires: requires.map(|required| QuestId(required.to_owned())),
        actor_kind: None,
        threshold,
        title: id.to_owned(),
        description: format!("do {id}"),
        completed_text: format!("{id} done"),
    }
}

// The shipped config with a synthetic catalog; every quest pays 100.
pub(crate) fn catalog(quests: Vec<Quest>) -> ServerGameplayConfig {
    let mut config = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
    config.scoring.quest_completed = quests.iter().map(|quest| (quest.id.clone(), 100)).collect();
    config.quests = quests;
    config
}

// A logged-in player with every unlocked quest assigned; the login batch is
// discarded so the receiver only sees what the test triggers.
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
    info.connection.logged_in = true;
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
pub(crate) fn assignment_for(catalog: &QuestCatalog, board: &QuestBoard) -> Vec<NewQuest> {
    let (tx, mut rx) = unbounded_channel();
    let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
    info.connection.logged_in = true;
    let player = PlayerId(1);
    let mut players = PlayerMap::default();
    players.insert(player, info);
    assign_quests(&mut players, player, catalog, board);
    match rx.try_recv() {
        Ok(ServerToClient::Send(ServerMessage::QuestsAssigned(assigned))) => assigned.quests,
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
    messages
        .iter()
        .any(|msg| matches!(msg, ServerMessage::QuestCompleted(c) if c.id.0 == id))
}

pub(crate) fn progress_values(messages: &[ServerMessage], id: &str) -> Vec<u32> {
    messages
        .iter()
        .filter_map(|msg| match msg {
            ServerMessage::QuestProgress(p) if p.id.0 == id => Some(p.progress),
            _ => None,
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
        .filter_map(|msg| match msg {
            ServerMessage::QuestsAssigned(assigned) => Some(assigned.quests.iter().map(|q| q.id.0.clone())),
            _ => None,
        })
        .flatten()
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
