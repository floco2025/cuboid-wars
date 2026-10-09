use bevy::prelude::Entity;
use crossbeam_channel::{Receiver, unbounded};

use super::{
    QuestBoard, QuestCatalog, QuestEvent, assign_quests, complete_quest, recheck_everyone_quests, record_event,
};
use crate::{
    config::{Quest, QuestKind, ServerGameplayConfig, fixtures},
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

pub(crate) fn catalog(quests: Vec<Quest>) -> ServerGameplayConfig {
    let mut config = fixtures::server_config();
    config.quests = quests;
    config
}

// A fresh board over `quests` and the players who joined it.
pub(crate) struct Quests {
    pub(crate) config: ServerGameplayConfig,
    pub(crate) catalog: QuestCatalog,
    pub(crate) board: QuestBoard,
    pub(crate) players: PlayerMap,
}

impl Quests {
    pub(crate) fn new(quests: Vec<Quest>) -> Self {
        let config = catalog(quests);
        let catalog = QuestCatalog::from_config(&config);
        let board = QuestBoard::from_catalog(&catalog, None);
        Self {
            config,
            catalog,
            board,
            players: PlayerMap::default(),
        }
    }

    // An active player with every unlocked quest assigned; the assignment
    // batch is discarded so the receiver only sees what the test triggers.
    pub(crate) fn join(&mut self, id: u32) -> Receiver<ServerMessage> {
        self.join_with(id, false)
    }

    pub(crate) fn join_dead(&mut self, id: u32) -> Receiver<ServerMessage> {
        self.join_with(id, true)
    }

    fn join_with(&mut self, id: u32, dead: bool) -> Receiver<ServerMessage> {
        let (tx, rx) = unbounded();
        let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
        info.connection.logged_in = true;
        info.connection.name = format!("P{id}");
        if dead {
            info.begin_respawn(2.0);
        }
        let player = PlayerId(id);
        self.players.insert(player, info);
        assign_quests(&mut self.players, player, &self.catalog, &self.board);
        while rx.try_recv().is_ok() {}
        rx
    }

    pub(crate) fn record(&mut self, event: QuestEvent) {
        record_event(
            &mut self.players,
            &mut self.board,
            &self.catalog,
            &self.config.feed,
            event,
        );
    }

    pub(crate) fn gold(&mut self, id: u32) {
        self.record(QuestEvent::GoldCollected { player: PlayerId(id) });
    }

    pub(crate) fn kill(&mut self, id: u32, kind: &str) {
        self.record(QuestEvent::ActorKilled {
            player: PlayerId(id),
            kind,
        });
    }

    pub(crate) fn complete(&mut self, quest: &str, targets: &[PlayerId]) -> usize {
        let quest = self
            .catalog
            .get(&QuestId(quest.to_owned()))
            .expect("quest missing from the catalog");
        complete_quest(
            &mut self.players,
            &mut self.board,
            &self.catalog,
            &self.config.feed,
            quest,
            targets,
        )
    }

    pub(crate) fn recheck(&mut self) {
        recheck_everyone_quests(&mut self.players, &mut self.board, &self.catalog, &self.config.feed);
    }

    // What a fresh player would be assigned right now.
    pub(crate) fn assignment(&self) -> Vec<QuestState> {
        let (tx, rx) = unbounded();
        let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
        info.connection.logged_in = true;
        let player = PlayerId(1);
        let mut players = PlayerMap::default();
        players.insert(player, info);
        assign_quests(&mut players, player, &self.catalog, &self.board);
        match rx.try_recv() {
            Ok(ServerMessage::QuestUpdates(message)) => message
                .updates
                .into_iter()
                .filter_map(|update| (update.reason == QuestUpdateReason::Assigned).then_some(update.quest))
                .collect(),
            _ => Vec::new(),
        }
    }

    pub(crate) fn score(&self, id: u32) -> i32 {
        self.players.get(&PlayerId(id)).expect("player tracked").session.score
    }

    pub(crate) fn own_progress(&self, id: u32, quest: &str) -> u32 {
        self.players
            .get(&PlayerId(id))
            .expect("player tracked")
            .session
            .quest_states[&QuestId(quest.to_owned())]
            .own_progress()
            .expect("quest has own progress")
    }
}

pub(crate) fn drain(receiver: &mut Receiver<ServerMessage>) -> Vec<ServerMessage> {
    let mut messages = Vec::new();
    while let Ok(message) = receiver.try_recv() {
        messages.push(message);
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
