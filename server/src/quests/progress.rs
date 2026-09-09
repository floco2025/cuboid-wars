use crate::{
    config::{FeedConfig, Quest, QuestKind},
    network::{FeedAudience, FeedEvent, ServerToClient, emit_feed},
    players::{PlayerInfo, PlayerMap, PlayerQuestState},
};
use common::protocol::{
    PlayerId, QuestId, QuestScope, QuestState, QuestStateProgress, QuestStatus, QuestUpdate, QuestUpdateReason,
    SQuestUpdates, ServerMessage,
};

use super::{QuestBoard, QuestCatalog, catalog::CatalogQuest, resources::everyone_count};

pub enum QuestEvent<'a> {
    GoldCollected { player: PlayerId },
    ActorKilled { player: PlayerId, kind: &'a str },
    FireworksStarted,
}

impl QuestEvent<'_> {
    fn kind(&self) -> QuestKind {
        match self {
            Self::GoldCollected { .. } => QuestKind::Gold,
            Self::ActorKilled { .. } => QuestKind::ActorKills,
            Self::FireworksStarted => QuestKind::Fireworks,
        }
    }

    fn matches(&self, quest: &Quest) -> bool {
        if quest.kind != self.kind() {
            return false;
        }
        match self {
            Self::GoldCollected { .. } | Self::FireworksStarted => true,
            Self::ActorKilled { kind, .. } => quest.actor_kind.as_deref().is_none_or(|want| want == *kind),
        }
    }

    fn player(&self) -> Option<PlayerId> {
        match self {
            Self::GoldCollected { player } | Self::ActorKilled { player, .. } => Some(*player),
            Self::FireworksStarted => None,
        }
    }
}

pub fn record_event(
    players: &mut PlayerMap,
    board: &mut QuestBoard,
    catalog: &QuestCatalog,
    feed: &FeedConfig,
    event: QuestEvent,
) {
    for quest in affected(catalog, board, |quest| event.matches(quest)) {
        match (event.player(), quest.scope) {
            (Some(player), QuestScope::Individual) => advance_individual(players, board, feed, player, quest),
            (Some(player), QuestScope::Everyone) => {
                advance_everyone(players, board, catalog, feed, player, quest);
            }
            (_, QuestScope::Shared) => advance_shared(players, board, catalog, feed, quest),
            (None, QuestScope::Individual | QuestScope::Everyone) => {
                panic!(
                    "world-event quest {:?} is not shared (config validation missed it)",
                    quest.id.0
                );
            }
        }
    }
}

// Decided up front so a quest this event unlocks can't also consume it.
fn affected<'a>(
    catalog: &'a QuestCatalog,
    board: &QuestBoard,
    matches: impl Fn(&Quest) -> bool,
) -> Vec<&'a CatalogQuest> {
    catalog
        .iter()
        .filter(|quest| matches(quest) && board.is_unlocked(&quest.id) && !board.is_completed(&quest.id))
        .collect()
}

// Adds one to the player's own count; `None` when the player
// is gone, isn't assigned, or already finished their part.
fn bump_own_progress(players: &mut PlayerMap, actor: PlayerId, quest: &Quest) -> Option<u32> {
    let current = players
        .get(&actor)?
        .session
        .quest_states
        .get(&quest.id)?
        .own_progress()?;
    raise_own_progress(players, actor, quest, current.saturating_add(1))
}

// Raises the player's own count to `to` (capped at the threshold, never
// lowered); `None` when nothing changed.
fn raise_own_progress(players: &mut PlayerMap, actor: PlayerId, quest: &Quest, to: u32) -> Option<u32> {
    let info = players.get_mut(&actor)?;
    let progress = info.session.quest_states.get_mut(&quest.id)?.own_progress_mut()?;
    let to = to.min(quest.threshold);
    if *progress >= to {
        return None;
    }
    *progress = to;
    Some(to)
}

fn advance_individual(
    players: &mut PlayerMap,
    board: &QuestBoard,
    feed: &FeedConfig,
    actor: PlayerId,
    quest: &CatalogQuest,
) {
    if let Some(progress) = bump_own_progress(players, actor, quest) {
        settle_individual(players, board, feed, actor, quest, progress);
    }
}

fn settle_individual(
    players: &mut PlayerMap,
    board: &QuestBoard,
    feed: &FeedConfig,
    actor: PlayerId,
    quest: &CatalogQuest,
    progress: u32,
) {
    if progress < quest.threshold {
        send_quest_update(players, board, actor, quest, QuestUpdateReason::Progressed);
        return;
    }
    let Some(info) = players.get_mut(&actor) else {
        return;
    };
    info.session.score += quest.points;
    send_quest_update(players, board, actor, quest, QuestUpdateReason::Completed);
    emit_feed(
        players,
        feed,
        FeedAudience::Everyone,
        FeedEvent::QuestCompleted {
            name: players.display_name(&actor),
            title: quest.title.clone(),
        },
    );
}

fn advance_everyone(
    players: &mut PlayerMap,
    board: &mut QuestBoard,
    catalog: &QuestCatalog,
    feed: &FeedConfig,
    actor: PlayerId,
    quest: &CatalogQuest,
) {
    if let Some(progress) = bump_own_progress(players, actor, quest) {
        settle_everyone(players, board, catalog, feed, actor, quest, progress);
    }
}

fn settle_everyone(
    players: &mut PlayerMap,
    board: &mut QuestBoard,
    catalog: &QuestCatalog,
    feed: &FeedConfig,
    actor: PlayerId,
    quest: &CatalogQuest,
    progress: u32,
) {
    if progress < quest.threshold {
        send_quest_update(players, board, actor, quest, QuestUpdateReason::Progressed);
        return;
    }
    let count = everyone_count(players, quest);
    if count.all_done() {
        complete_group(players, board, catalog, feed, quest);
    } else {
        send_quest_update(players, board, actor, quest, QuestUpdateReason::Progressed);
        emit_feed(
            players,
            feed,
            FeedAudience::Everyone,
            FeedEvent::EveryoneQuestPartDone {
                name: players.display_name(&actor),
                title: quest.title.clone(),
                players_done: count.players_done,
                players_total: count.players_total,
            },
        );
    }
}

fn advance_shared(
    players: &mut PlayerMap,
    board: &mut QuestBoard,
    catalog: &QuestCatalog,
    feed: &FeedConfig,
    quest: &CatalogQuest,
) {
    if board.add_shared_progress(quest) >= quest.threshold {
        complete_group(players, board, catalog, feed, quest);
    } else {
        send_group_update(players, board, quest, QuestUpdateReason::Progressed);
    }
}

fn complete_group(
    players: &mut PlayerMap,
    board: &mut QuestBoard,
    catalog: &QuestCatalog,
    feed: &FeedConfig,
    quest: &CatalogQuest,
) {
    if !board.finish_group(quest) {
        return;
    }
    for (_, info) in players.iter_mut() {
        if info.connection.logged_in {
            info.session.score += quest.points;
        }
    }
    send_group_update(players, board, quest, QuestUpdateReason::Completed);
    emit_feed(
        players,
        feed,
        FeedAudience::Everyone,
        FeedEvent::GroupQuestCompleted {
            title: quest.title.clone(),
        },
    );
    unlock_dependents(players, board, catalog, &quest.id);
}

fn unlock_dependents(players: &mut PlayerMap, board: &mut QuestBoard, catalog: &QuestCatalog, completed: &QuestId) {
    for quest in catalog.dependents(completed) {
        if !board.is_unlocked(&quest.id) {
            unlock(players, board, quest);
        }
    }
}

fn unlock(players: &mut PlayerMap, board: &mut QuestBoard, quest: &CatalogQuest) {
    board.unlock(&quest.id);
    let assigned: Vec<PlayerId> = players
        .iter_mut()
        .filter_map(|(id, info)| (info.connection.logged_in && assign_state(info, quest, board)).then_some(*id))
        .collect();
    for player in assigned {
        let new_quest = quest_state(players, player, quest, board);
        if let Some(info) = players.get(&player) {
            notify_assigned(info, vec![new_quest]);
        }
    }
}

// Admin: open a locked quest now, prerequisite or not.
pub fn unlock_quest(players: &mut PlayerMap, board: &mut QuestBoard, catalog: &QuestCatalog, id: &QuestId) {
    if let Some(quest) = catalog.get(id)
        && !board.is_unlocked(id)
    {
        unlock(players, board, quest);
    }
}

// Admin: finish `quest` for `targets` — their own parts for `individual` /
// `everyone` (the group completes once every part is done, as usual), the
// group outright for `shared`. Returns how many own parts this finished.
pub fn complete_quest(
    players: &mut PlayerMap,
    board: &mut QuestBoard,
    catalog: &QuestCatalog,
    feed: &FeedConfig,
    quest: &CatalogQuest,
    targets: &[PlayerId],
) -> usize {
    let mut finished = 0;
    match quest.scope {
        QuestScope::Individual => {
            for &actor in targets {
                if let Some(progress) = raise_own_progress(players, actor, quest, quest.threshold) {
                    settle_individual(players, board, feed, actor, quest, progress);
                    finished += 1;
                }
            }
        }
        QuestScope::Everyone => {
            for &actor in targets {
                if board.is_completed(&quest.id) {
                    break;
                }
                if let Some(progress) = raise_own_progress(players, actor, quest, quest.threshold) {
                    settle_everyone(players, board, catalog, feed, actor, quest, progress);
                    finished += 1;
                }
            }
        }
        QuestScope::Shared => {
            complete_group(players, board, catalog, feed, quest);
        }
    }
    finished
}

// Every unlocked quest the player doesn't have yet, sent as one batch (no
// points for late joiners to a completed group quest).
pub fn assign_quests(players: &mut PlayerMap, player: PlayerId, catalog: &QuestCatalog, board: &QuestBoard) {
    let new_quests = assign_quest_states(players, player, catalog, board);
    if new_quests.is_empty() {
        return;
    }
    if let Some(info) = players.get(&player) {
        notify_assigned(info, new_quests);
    }
}

fn assign_quest_states(
    players: &mut PlayerMap,
    player: PlayerId,
    catalog: &QuestCatalog,
    board: &QuestBoard,
) -> Vec<QuestState> {
    let Some(info) = players.get_mut(&player) else {
        return Vec::new();
    };
    let mut new_quests = Vec::new();
    for quest in catalog.iter().filter(|quest| board.is_unlocked(&quest.id)) {
        if assign_state(info, quest, board) {
            new_quests.push(quest);
        }
    }
    if new_quests.is_empty() {
        return Vec::new();
    }
    new_quests
        .into_iter()
        .map(|quest| quest_state(players, player, quest, board))
        .collect()
}

fn assign_state(player_info: &mut PlayerInfo, quest: &Quest, board: &QuestBoard) -> bool {
    if player_info.session.quest_states.contains_key(&quest.id) {
        return false;
    }
    let progress = if quest.scope.is_group() && board.is_completed(&quest.id) {
        quest.threshold
    } else {
        0
    };
    player_info
        .session
        .quest_states
        .insert(quest.id.clone(), PlayerQuestState::new(quest.scope, progress));
    true
}

fn quest_state(players: &PlayerMap, player: PlayerId, quest: &CatalogQuest, board: &QuestBoard) -> QuestState {
    let own_progress = players
        .get(&player)
        .and_then(|info| info.session.quest_states.get(&quest.id))
        .and_then(|state| state.own_progress())
        .unwrap_or(0);
    let progress = match quest.scope {
        QuestScope::Individual => QuestStateProgress::Individual { progress: own_progress },
        QuestScope::Shared => QuestStateProgress::Shared {
            progress: board.shared_progress(&quest.id),
        },
        QuestScope::Everyone => {
            let count = everyone_count(players, quest);
            QuestStateProgress::Everyone {
                progress: own_progress,
                players_done: count.players_done,
                players_total: count.players_total,
            }
        }
    };
    QuestState {
        id: quest.id.clone(),
        title: quest.title.clone(),
        description: quest.description.clone(),
        completed_text: quest.completed_text.clone(),
        threshold: quest.threshold,
        scope: quest.scope,
        order: quest.order,
        status: QuestStatus {
            completed: match quest.scope {
                QuestScope::Individual => own_progress >= quest.threshold,
                QuestScope::Shared | QuestScope::Everyone => board.is_completed(&quest.id),
            },
            progress,
        },
    }
}

fn notify_assigned(info: &PlayerInfo, quests: Vec<QuestState>) {
    let updates = quests
        .into_iter()
        .map(|quest| QuestUpdate {
            reason: QuestUpdateReason::Assigned,
            quest,
        })
        .collect();
    send(info, ServerMessage::QuestUpdates(SQuestUpdates { updates }));
}

// Any change to the active-player set can finish an `everyone` quest whose last
// holdout is gone.
pub fn recheck_everyone_quests(
    players: &mut PlayerMap,
    board: &mut QuestBoard,
    catalog: &QuestCatalog,
    feed: &FeedConfig,
) {
    for quest in catalog.iter() {
        if quest.scope != QuestScope::Everyone || !board.is_unlocked(&quest.id) || board.is_completed(&quest.id) {
            continue;
        }
        if everyone_count(players, quest).all_done() {
            complete_group(players, board, catalog, feed, quest);
        }
    }
}

fn send_quest_update(
    players: &PlayerMap,
    board: &QuestBoard,
    player: PlayerId,
    quest: &CatalogQuest,
    reason: QuestUpdateReason,
) {
    let quest = quest_state(players, player, quest, board);
    if let Some(info) = players.get(&player) {
        send(
            info,
            ServerMessage::QuestUpdates(SQuestUpdates {
                updates: vec![QuestUpdate { reason, quest }],
            }),
        );
    }
}

fn send_group_update(players: &PlayerMap, board: &QuestBoard, quest: &CatalogQuest, reason: QuestUpdateReason) {
    let recipients: Vec<PlayerId> = players
        .iter()
        .filter_map(|(id, info)| info.connection.logged_in.then_some(*id))
        .collect();
    for player in recipients {
        send_quest_update(players, board, player, quest, reason);
    }
}

fn send(info: &PlayerInfo, message: ServerMessage) {
    let _ = info.connection.channel.send(ServerToClient::Send(message));
}
