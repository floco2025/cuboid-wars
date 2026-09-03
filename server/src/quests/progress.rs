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
    CookieCollected { player: PlayerId },
    ActorKilled { player: PlayerId, kind: &'a str },
    FireworksStarted,
}

impl QuestEvent<'_> {
    fn kind(&self) -> QuestKind {
        match self {
            Self::CookieCollected { .. } => QuestKind::Cookies,
            Self::ActorKilled { .. } => QuestKind::ActorKills,
            Self::FireworksStarted => QuestKind::Fireworks,
        }
    }

    fn matches(&self, quest: &Quest) -> bool {
        if quest.kind != self.kind() {
            return false;
        }
        match self {
            Self::CookieCollected { .. } | Self::FireworksStarted => true,
            Self::ActorKilled { kind, .. } => quest.actor_kind.as_deref().is_none_or(|want| want == *kind),
        }
    }

    fn player(&self) -> Option<PlayerId> {
        match self {
            Self::CookieCollected { player } | Self::ActorKilled { player, .. } => Some(*player),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quests::test_support::{
        assigned_ids, assignment_for, catalog, completed, drain, feed_lines, join, join_with, own_progress,
        progress_values, quest, score,
    };

    fn cookie(players: &mut PlayerMap, board: &mut QuestBoard, catalog: &QuestCatalog, feed: &FeedConfig, id: u32) {
        record_event(
            players,
            board,
            catalog,
            feed,
            QuestEvent::CookieCollected { player: PlayerId(id) },
        );
    }

    fn kill(
        players: &mut PlayerMap,
        board: &mut QuestBoard,
        catalog: &QuestCatalog,
        feed: &FeedConfig,
        id: u32,
        kind: &str,
    ) {
        record_event(
            players,
            board,
            catalog,
            feed,
            QuestEvent::ActorKilled {
                player: PlayerId(id),
                kind,
            },
        );
    }

    fn id(quest: &str) -> QuestId {
        QuestId(quest.to_owned())
    }

    fn initial_progress(quest: &QuestState) -> u32 {
        match &quest.status.progress {
            QuestStateProgress::Individual { progress }
            | QuestStateProgress::Shared { progress }
            | QuestStateProgress::Everyone { progress, .. } => *progress,
        }
    }

    #[test]
    fn individual_progress_and_completion_stay_per_player() {
        let config = catalog(vec![quest("gold", QuestKind::Cookies, QuestScope::Individual, 2, None)]);
        let quest_catalog = QuestCatalog::from_config(&config);
        let mut board = QuestBoard::from_catalog(&quest_catalog);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &quest_catalog, &board);
        let mut bob = join(&mut players, 2, &quest_catalog, &board);

        cookie(&mut players, &mut board, &quest_catalog, &config.feed, 1);
        cookie(&mut players, &mut board, &quest_catalog, &config.feed, 1);
        cookie(&mut players, &mut board, &quest_catalog, &config.feed, 1);

        let alice_messages = drain(&mut alice);
        assert_eq!(
            progress_values(&alice_messages, "gold"),
            [1],
            "the third cookie is past the threshold"
        );
        assert!(completed(&alice_messages, "gold"));
        assert_eq!(score(&players, 1), 100);
        let bob_messages = drain(&mut bob);
        assert!(!completed(&bob_messages, "gold"));
        assert_eq!(feed_lines(&bob_messages), ["P1 completed gold"]);
        assert_eq!(own_progress(&players, 2, "gold"), 0);
        assert_eq!(score(&players, 2), 0);
    }

    #[test]
    fn actor_kill_respects_kind_filter() {
        let mut sentries = quest("sentries", QuestKind::ActorKills, QuestScope::Individual, 2, None);
        sentries.actor_kind = Some("sentry".to_owned());
        let config = catalog(vec![sentries]);
        let quest_catalog = QuestCatalog::from_config(&config);
        let mut board = QuestBoard::from_catalog(&quest_catalog);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &quest_catalog, &board);

        kill(&mut players, &mut board, &quest_catalog, &config.feed, 1, "zapper");
        assert_eq!(own_progress(&players, 1, "sentries"), 0);
        kill(&mut players, &mut board, &quest_catalog, &config.feed, 1, "sentry");
        assert_eq!(own_progress(&players, 1, "sentries"), 1);
    }

    #[test]
    fn shared_quest_pools_progress_and_scores_everyone_once() {
        let config = catalog(vec![quest("hunt", QuestKind::ActorKills, QuestScope::Shared, 2, None)]);
        let quest_catalog = QuestCatalog::from_config(&config);
        let mut board = QuestBoard::from_catalog(&quest_catalog);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &quest_catalog, &board);
        let mut bob = join(&mut players, 2, &quest_catalog, &board);

        kill(&mut players, &mut board, &quest_catalog, &config.feed, 1, "sentry");
        assert_eq!(board.shared_progress(&id("hunt")), 1);
        assert!(!board.is_completed(&id("hunt")));

        kill(&mut players, &mut board, &quest_catalog, &config.feed, 2, "sentry");
        assert!(board.is_completed(&id("hunt")));
        for rx in [&mut alice, &mut bob] {
            let messages = drain(rx);
            assert!(completed(&messages, "hunt"));
            assert_eq!(progress_values(&messages, "hunt"), [1]);
            assert_eq!(feed_lines(&messages), ["Everyone completed hunt"]);
        }
        assert_eq!((score(&players, 1), score(&players, 2)), (100, 100));

        kill(&mut players, &mut board, &quest_catalog, &config.feed, 1, "sentry");
        assert!(drain(&mut alice).is_empty(), "latched: nothing after completion");
        assert_eq!(score(&players, 1), 100);
    }

    #[test]
    fn shared_quest_counts_events_from_a_departed_player() {
        let config = catalog(vec![quest("hunt", QuestKind::ActorKills, QuestScope::Shared, 2, None)]);
        let quest_catalog = QuestCatalog::from_config(&config);
        let mut board = QuestBoard::from_catalog(&quest_catalog);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &quest_catalog, &board);
        let _bob = join(&mut players, 2, &quest_catalog, &board);
        players.remove(&PlayerId(1));

        kill(&mut players, &mut board, &quest_catalog, &config.feed, 1, "sentry");

        assert_eq!(
            board.shared_progress(&id("hunt")),
            1,
            "the pool doesn't care who is still here"
        );
    }

    #[test]
    fn everyone_quest_completes_when_the_last_player_reaches_the_threshold() {
        let config = catalog(vec![quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None)]);
        let quest_catalog = QuestCatalog::from_config(&config);
        let mut board = QuestBoard::from_catalog(&quest_catalog);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &quest_catalog, &board);
        let mut bob = join(&mut players, 2, &quest_catalog, &board);

        cookie(&mut players, &mut board, &quest_catalog, &config.feed, 1);
        let alice_messages = drain(&mut alice);
        assert_eq!(progress_values(&alice_messages, "gold"), [1]);
        assert!(!completed(&alice_messages, "gold"));
        assert_eq!(feed_lines(&drain(&mut bob)), ["P1 finished gold (1/2 players)"]);

        cookie(&mut players, &mut board, &quest_catalog, &config.feed, 2);
        for rx in [&mut alice, &mut bob] {
            let messages = drain(rx);
            assert!(completed(&messages, "gold"));
            let lines = feed_lines(&messages);
            assert_eq!(lines, ["Everyone completed gold"]);
        }
        assert!(board.is_completed(&id("gold")));
        assert_eq!((score(&players, 1), score(&players, 2)), (100, 100));
    }

    #[test]
    fn everyone_quest_waits_for_a_dead_holdout() {
        let config = catalog(vec![quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None)]);
        let quest_catalog = QuestCatalog::from_config(&config);
        let mut board = QuestBoard::from_catalog(&quest_catalog);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &quest_catalog, &board);
        let _ghost = join_with(&mut players, 2, &quest_catalog, &board, true);

        cookie(&mut players, &mut board, &quest_catalog, &config.feed, 1);
        assert!(
            !board.is_completed(&id("gold")),
            "a dead player still counts toward everyone"
        );

        cookie(&mut players, &mut board, &quest_catalog, &config.feed, 2);
        assert!(board.is_completed(&id("gold")));
    }

    #[test]
    fn late_joiner_raises_the_denominator() {
        let config = catalog(vec![quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None)]);
        let quest_catalog = QuestCatalog::from_config(&config);
        let mut board = QuestBoard::from_catalog(&quest_catalog);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &quest_catalog, &board);
        let _bob = join(&mut players, 2, &quest_catalog, &board);
        cookie(&mut players, &mut board, &quest_catalog, &config.feed, 1);
        drain(&mut alice);

        let _carol = join(&mut players, 3, &quest_catalog, &board);
        cookie(&mut players, &mut board, &quest_catalog, &config.feed, 2);
        assert!(!board.is_completed(&id("gold")));
        assert_eq!(feed_lines(&drain(&mut alice)), ["P2 finished gold (2/3 players)"]);

        cookie(&mut players, &mut board, &quest_catalog, &config.feed, 3);
        assert!(board.is_completed(&id("gold")));
    }

    #[test]
    fn recheck_completes_when_the_leaver_was_the_holdout() {
        let config = catalog(vec![quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None)]);
        let quest_catalog = QuestCatalog::from_config(&config);
        let mut board = QuestBoard::from_catalog(&quest_catalog);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &quest_catalog, &board);
        let _bob = join(&mut players, 2, &quest_catalog, &board);
        cookie(&mut players, &mut board, &quest_catalog, &config.feed, 1);
        drain(&mut alice);

        players.remove(&PlayerId(2));
        recheck_everyone_quests(&mut players, &mut board, &quest_catalog, &config.feed);

        assert!(board.is_completed(&id("gold")));
        assert!(completed(&drain(&mut alice), "gold"));
        assert_eq!(score(&players, 1), 100);
    }

    #[test]
    fn recheck_completes_nothing_when_the_only_finisher_left() {
        let config = catalog(vec![quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None)]);
        let quest_catalog = QuestCatalog::from_config(&config);
        let mut board = QuestBoard::from_catalog(&quest_catalog);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &quest_catalog, &board);
        let _bob = join(&mut players, 2, &quest_catalog, &board);
        cookie(&mut players, &mut board, &quest_catalog, &config.feed, 1);

        players.remove(&PlayerId(1));
        recheck_everyone_quests(&mut players, &mut board, &quest_catalog, &config.feed);

        assert!(!board.is_completed(&id("gold")));
    }

    #[test]
    fn recheck_on_an_empty_server_completes_nothing() {
        let config = catalog(vec![quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None)]);
        let quest_catalog = QuestCatalog::from_config(&config);
        let mut board = QuestBoard::from_catalog(&quest_catalog);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &quest_catalog, &board);
        players.remove(&PlayerId(1));

        recheck_everyone_quests(&mut players, &mut board, &quest_catalog, &config.feed);

        assert!(!board.is_completed(&id("gold")));
    }

    #[test]
    fn group_completion_unlocks_dependents_and_assigns_them_to_everyone() {
        let config = catalog(vec![
            quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None),
            quest("show", QuestKind::Fireworks, QuestScope::Shared, 1, Some("gold")),
        ]);
        let quest_catalog = QuestCatalog::from_config(&config);
        let mut board = QuestBoard::from_catalog(&quest_catalog);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &quest_catalog, &board);
        let mut bob = join(&mut players, 2, &quest_catalog, &board);
        assert!(
            !players
                .get(&PlayerId(1))
                .expect("alice")
                .session
                .quest_states
                .contains_key(&id("show"))
        );

        cookie(&mut players, &mut board, &quest_catalog, &config.feed, 1);
        cookie(&mut players, &mut board, &quest_catalog, &config.feed, 2);

        assert!(board.is_unlocked(&id("show")));
        for rx in [&mut alice, &mut bob] {
            assert_eq!(assigned_ids(&drain(rx)), ["show"]);
        }
        assert!(
            players
                .get(&PlayerId(1))
                .expect("alice")
                .session
                .quest_states
                .contains_key(&id("show"))
        );

        // A late joiner gets the completed prerequisite as completed and the
        // unlocked quest fresh — and no points.
        let assigned = assignment_for(&quest_catalog, &board);
        let ids: Vec<&str> = assigned.iter().map(|q| q.id.0.as_str()).collect();
        assert_eq!(ids, ["gold", "show"]);
        assert!(assigned[0].status.completed);
        assert_eq!(initial_progress(&assigned[0]), 1);
        assert!(matches!(
            &assigned[0].status.progress,
            QuestStateProgress::Everyone {
                players_done,
                players_total,
                ..
            } if (*players_done, *players_total) == (1, 1)
        ));
        assert_eq!(initial_progress(&assigned[1]), 0);
        let _carol = join(&mut players, 3, &quest_catalog, &board);
        assert_eq!(score(&players, 3), 0);
    }

    #[test]
    fn requires_chain_unlocks_one_step_at_a_time() {
        let config = catalog(vec![
            quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None),
            quest("bonus", QuestKind::Cookies, QuestScope::Shared, 1, Some("gold")),
            quest("later", QuestKind::Cookies, QuestScope::Shared, 1, Some("bonus")),
        ]);
        let quest_catalog = QuestCatalog::from_config(&config);
        let mut board = QuestBoard::from_catalog(&quest_catalog);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &quest_catalog, &board);

        cookie(&mut players, &mut board, &quest_catalog, &config.feed, 1);
        assert!(board.is_completed(&id("gold")));
        assert!(board.is_unlocked(&id("bonus")) && !board.is_unlocked(&id("later")));
        assert_eq!(
            board.shared_progress(&id("bonus")),
            0,
            "the unlocking event doesn't feed what it unlocked"
        );

        cookie(&mut players, &mut board, &quest_catalog, &config.feed, 1);
        assert!(board.is_completed(&id("bonus")));
        assert!(board.is_unlocked(&id("later")) && !board.is_completed(&id("later")));

        cookie(&mut players, &mut board, &quest_catalog, &config.feed, 1);
        assert!(board.is_completed(&id("later")));
    }

    #[test]
    fn world_event_only_hits_its_kind() {
        let config = catalog(vec![
            quest("show", QuestKind::Fireworks, QuestScope::Shared, 1, None),
            quest("gold", QuestKind::Cookies, QuestScope::Individual, 5, None),
        ]);
        let quest_catalog = QuestCatalog::from_config(&config);
        let mut board = QuestBoard::from_catalog(&quest_catalog);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &quest_catalog, &board);

        record_event(
            &mut players,
            &mut board,
            &quest_catalog,
            &config.feed,
            QuestEvent::FireworksStarted,
        );

        assert!(board.is_completed(&id("show")));
        assert_eq!(own_progress(&players, 1, "gold"), 0);
    }

    #[test]
    fn dead_but_logged_in_players_are_credited_at_group_completion() {
        let config = catalog(vec![quest("show", QuestKind::Fireworks, QuestScope::Shared, 1, None)]);
        let quest_catalog = QuestCatalog::from_config(&config);
        let mut board = QuestBoard::from_catalog(&quest_catalog);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &quest_catalog, &board);
        let mut ghost = join_with(&mut players, 2, &quest_catalog, &board, true);

        record_event(
            &mut players,
            &mut board,
            &quest_catalog,
            &config.feed,
            QuestEvent::FireworksStarted,
        );

        for rx in [&mut alice, &mut ghost] {
            assert!(completed(&drain(rx), "show"));
        }
        assert_eq!((score(&players, 1), score(&players, 2)), (100, 100));
    }

    #[test]
    fn assign_quests_skips_locked_and_seeds_completed_group_quests() {
        let config = catalog(vec![
            quest("gold", QuestKind::Cookies, QuestScope::Everyone, 3, None),
            quest("show", QuestKind::Fireworks, QuestScope::Shared, 1, Some("gold")),
            quest("later", QuestKind::Cookies, QuestScope::Shared, 1, Some("show")),
        ]);
        let quest_catalog = QuestCatalog::from_config(&config);
        let mut board = QuestBoard::from_catalog(&quest_catalog);
        board.finish_group(quest_catalog.get(&id("gold")).expect("gold quest missing"));
        board.unlock(&id("show"));

        let assigned = assignment_for(&quest_catalog, &board);

        let ids: Vec<&str> = assigned.iter().map(|q| q.id.0.as_str()).collect();
        assert_eq!(ids, ["gold", "show"]);
        assert_eq!(
            initial_progress(&assigned[0]),
            3,
            "completed group quests arrive at the threshold"
        );
        assert!(assigned[0].status.completed);
    }

    #[test]
    fn joining_against_a_live_pooled_counter_sees_the_pool() {
        let config = catalog(vec![quest("pool", QuestKind::Cookies, QuestScope::Shared, 5, None)]);
        let quest_catalog = QuestCatalog::from_config(&config);
        let mut board = QuestBoard::from_catalog(&quest_catalog);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &quest_catalog, &board);
        for _ in 0..3 {
            cookie(&mut players, &mut board, &quest_catalog, &config.feed, 1);
        }

        let assigned = assignment_for(&quest_catalog, &board);
        assert_eq!(initial_progress(&assigned[0]), 3);
    }

    #[test]
    fn admin_completion_finishes_individual_quests_for_the_targets_only() {
        let config = catalog(vec![quest("gold", QuestKind::Cookies, QuestScope::Individual, 3, None)]);
        let quest_catalog = QuestCatalog::from_config(&config);
        let mut board = QuestBoard::from_catalog(&quest_catalog);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &quest_catalog, &board);
        let _bob = join(&mut players, 2, &quest_catalog, &board);
        let gold = quest_catalog.get(&id("gold")).expect("gold quest missing");

        assert_eq!(
            complete_quest(
                &mut players,
                &mut board,
                &quest_catalog,
                &config.feed,
                gold,
                &[PlayerId(1)]
            ),
            1
        );

        let messages = drain(&mut alice);
        assert!(progress_values(&messages, "gold").is_empty());
        assert!(completed(&messages, "gold"));
        assert_eq!(score(&players, 1), 100);
        assert_eq!(own_progress(&players, 2, "gold"), 0);
        assert_eq!(
            complete_quest(
                &mut players,
                &mut board,
                &quest_catalog,
                &config.feed,
                gold,
                &[PlayerId(1)]
            ),
            0,
            "already finished"
        );
    }

    #[test]
    fn admin_completion_of_an_everyone_quest_completes_the_group_once_every_part_is_done() {
        let config = catalog(vec![quest("gold", QuestKind::Cookies, QuestScope::Everyone, 3, None)]);
        let quest_catalog = QuestCatalog::from_config(&config);
        let mut board = QuestBoard::from_catalog(&quest_catalog);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &quest_catalog, &board);
        let _bob = join(&mut players, 2, &quest_catalog, &board);
        let gold = quest_catalog.get(&id("gold")).expect("gold quest missing");

        assert_eq!(
            complete_quest(
                &mut players,
                &mut board,
                &quest_catalog,
                &config.feed,
                gold,
                &[PlayerId(1)]
            ),
            1
        );
        assert!(!board.is_completed(&id("gold")));
        assert_eq!(feed_lines(&drain(&mut alice)), ["P1 finished gold (1/2 players)"]);

        assert_eq!(
            complete_quest(
                &mut players,
                &mut board,
                &quest_catalog,
                &config.feed,
                gold,
                &[PlayerId(1), PlayerId(2)]
            ),
            1,
            "only Bob's part was still open"
        );
        assert!(board.is_completed(&id("gold")));
        assert_eq!(score(&players, 1), 100);
        assert_eq!(score(&players, 2), 100);
    }

    #[test]
    fn admin_unlock_and_shared_completion_bypass_the_prerequisite() {
        let config = catalog(vec![
            quest("hunt", QuestKind::ActorKills, QuestScope::Shared, 5, None),
            quest("bonus", QuestKind::Cookies, QuestScope::Individual, 1, Some("hunt")),
        ]);
        let quest_catalog = QuestCatalog::from_config(&config);
        let mut board = QuestBoard::from_catalog(&quest_catalog);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &quest_catalog, &board);

        unlock_quest(&mut players, &mut board, &quest_catalog, &id("bonus"));
        assert!(board.is_unlocked(&id("bonus")));
        assert_eq!(assigned_ids(&drain(&mut alice)), ["bonus"]);

        assert_eq!(
            complete_quest(
                &mut players,
                &mut board,
                &quest_catalog,
                &config.feed,
                quest_catalog.get(&id("hunt")).expect("hunt quest missing"),
                &[]
            ),
            0,
            "a shared quest has no own parts"
        );
        assert!(board.is_completed(&id("hunt")));
        assert_eq!(board.shared_progress(&id("hunt")), 5);
        assert!(completed(&drain(&mut alice), "hunt"));
        assert_eq!(score(&players, 1), 100);

        complete_quest(
            &mut players,
            &mut board,
            &quest_catalog,
            &config.feed,
            quest_catalog.get(&id("hunt")).expect("hunt quest missing"),
            &[],
        );
        assert_eq!(score(&players, 1), 100);
        assert!(drain(&mut alice).is_empty());
    }
}
