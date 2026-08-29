use crate::{
    config::{Quest, QuestKind, ServerGameplayConfig},
    network::{ServerToClient, announce, broadcast_to_all},
    players::{PlayerInfo, PlayerMap},
};
use common::protocol::{
    FeedEvent, NewQuest, PlayerId, QuestId, QuestScope, SQuestCompleted, SQuestProgress, SQuestsAssigned, ServerMessage,
};

use super::{QuestBoard, resources::everyone_count};

// Something a player did.
pub enum PlayerQuestEvent<'a> {
    CookieCollected,
    ActorKilled { kind: &'a str },
}

// Something that happened to the world; only `shared` quests consume these
// (config validation).
#[derive(Clone, Copy)]
pub enum WorldQuestEvent {
    FireworksStarted,
}

impl PlayerQuestEvent<'_> {
    fn kind(&self) -> QuestKind {
        match self {
            Self::CookieCollected => QuestKind::Cookies,
            Self::ActorKilled { .. } => QuestKind::ActorKills,
        }
    }

    fn matches(&self, quest: &Quest) -> bool {
        if quest.kind != self.kind() {
            return false;
        }
        match self {
            Self::CookieCollected => true,
            Self::ActorKilled { kind } => quest.actor_kind.as_deref().is_none_or(|want| want == *kind),
        }
    }
}

impl WorldQuestEvent {
    fn kind(self) -> QuestKind {
        match self {
            Self::FireworksStarted => QuestKind::Fireworks,
        }
    }
}

pub fn record_player_event(
    players: &mut PlayerMap,
    board: &mut QuestBoard,
    config: &ServerGameplayConfig,
    actor: PlayerId,
    event: PlayerQuestEvent,
) {
    for quest in affected(config, board, |quest| event.matches(quest)) {
        match quest.scope {
            QuestScope::Individual => advance_individual(players, config, actor, quest),
            QuestScope::Everyone => advance_everyone(players, board, config, actor, quest),
            QuestScope::Shared => advance_shared(players, board, config, quest),
        }
    }
}

pub fn record_world_event(
    players: &mut PlayerMap,
    board: &mut QuestBoard,
    config: &ServerGameplayConfig,
    event: WorldQuestEvent,
) {
    for quest in affected(config, board, |quest| quest.kind == event.kind()) {
        assert!(
            quest.scope == QuestScope::Shared,
            "world-event quest {:?} is not shared (config validation missed it)",
            quest.id.0
        );
        advance_shared(players, board, config, quest);
    }
}

// Decided up front so a quest this event unlocks can't also consume it.
fn affected<'a>(
    config: &'a ServerGameplayConfig,
    board: &QuestBoard,
    matches: impl Fn(&Quest) -> bool,
) -> Vec<&'a Quest> {
    config
        .quests
        .iter()
        .filter(|quest| matches(quest) && board.is_unlocked(&quest.id) && !board.is_completed(&quest.id))
        .collect()
}

// Adds one to the player's own count and tells them; `None` when the player
// is gone, isn't assigned, or already finished their part.
fn bump_own_progress(players: &mut PlayerMap, actor: PlayerId, quest: &Quest) -> Option<u32> {
    let info = players.get_mut(&actor)?;
    let progress = info.quest_states.get_mut(&quest.id)?;
    if *progress >= quest.threshold {
        return None;
    }
    *progress = progress.saturating_add(1);
    let progress = *progress;
    send(info, progress_message(quest, progress));
    Some(progress)
}

fn advance_individual(players: &mut PlayerMap, config: &ServerGameplayConfig, actor: PlayerId, quest: &Quest) {
    let Some(progress) = bump_own_progress(players, actor, quest) else {
        return;
    };
    if progress < quest.threshold {
        return;
    }
    let Some(info) = players.get_mut(&actor) else {
        return;
    };
    info.score += quest_points(config, quest);
    send(info, completed_message(quest));
    announce(
        players,
        &config.feed,
        FeedEvent::QuestCompleted {
            name: players.display_name(&actor),
            title: quest.title.clone(),
        },
    );
}

fn advance_everyone(
    players: &mut PlayerMap,
    board: &mut QuestBoard,
    config: &ServerGameplayConfig,
    actor: PlayerId,
    quest: &Quest,
) {
    let Some(progress) = bump_own_progress(players, actor, quest) else {
        return;
    };
    if progress < quest.threshold {
        return;
    }
    let count = everyone_count(players, quest);
    if count.all_done() {
        complete_group(players, board, config, quest);
    } else {
        announce(
            players,
            &config.feed,
            FeedEvent::EveryoneQuestPartDone {
                name: players.display_name(&actor),
                title: quest.title.clone(),
                players_done: count.players_done,
                players_total: count.players_total,
            },
        );
    }
}

fn advance_shared(players: &mut PlayerMap, board: &mut QuestBoard, config: &ServerGameplayConfig, quest: &Quest) {
    if board.add_shared_progress(&quest.id) >= quest.threshold {
        complete_group(players, board, config, quest);
    }
}

fn complete_group(players: &mut PlayerMap, board: &mut QuestBoard, config: &ServerGameplayConfig, quest: &Quest) {
    board.latch_completed(&quest.id);
    let points = quest_points(config, quest);
    for (_, info) in players.iter_mut() {
        if info.logged_in {
            info.score += points;
        }
    }
    broadcast_to_all(players, completed_message(quest));
    announce(
        players,
        &config.feed,
        FeedEvent::GroupQuestCompleted {
            title: quest.title.clone(),
        },
    );
    unlock_dependents(players, board, config, &quest.id);
}

fn unlock_dependents(
    players: &mut PlayerMap,
    board: &mut QuestBoard,
    config: &ServerGameplayConfig,
    completed: &QuestId,
) {
    for (order, quest) in catalog(&config.quests) {
        if quest.requires.as_ref() != Some(completed) || board.is_unlocked(&quest.id) {
            continue;
        }
        board.unlock(&quest.id);
        for (_, info) in players.iter_mut() {
            if !info.logged_in {
                continue;
            }
            if let Some(new_quest) = assign_quest(info, quest, order, board) {
                notify_assigned(info, vec![new_quest]);
            }
        }
    }
}

// Every unlocked quest the player doesn't have yet, sent as one batch (no
// points for late joiners to a completed group quest).
pub fn assign_quests(player_info: &mut PlayerInfo, quests: &[Quest], board: &QuestBoard) {
    let new_quests: Vec<NewQuest> = catalog(quests)
        .filter(|(_, quest)| board.is_unlocked(&quest.id))
        .filter_map(|(order, quest)| assign_quest(player_info, quest, order, board))
        .collect();
    if !new_quests.is_empty() {
        notify_assigned(player_info, new_quests);
    }
}

// The catalog with each quest's display rank: its position in `gameplay.json`.
fn catalog(quests: &[Quest]) -> impl Iterator<Item = (u32, &Quest)> {
    quests.iter().enumerate().map(|(index, quest)| (index as u32, quest))
}

fn assign_quest(player_info: &mut PlayerInfo, quest: &Quest, order: u32, board: &QuestBoard) -> Option<NewQuest> {
    if player_info.quest_states.contains_key(&quest.id) {
        return None;
    }
    player_info.quest_states.insert(quest.id.clone(), 0);
    let progress = if board.is_completed(&quest.id) {
        quest.threshold
    } else if quest.scope == QuestScope::Shared {
        board.shared_progress(&quest.id)
    } else {
        0
    };
    Some(NewQuest {
        id: quest.id.clone(),
        scope: quest.scope,
        title: quest.title.clone(),
        description: quest.description.clone(),
        progress,
        threshold: quest.threshold,
        order,
    })
}

fn notify_assigned(info: &PlayerInfo, quests: Vec<NewQuest>) {
    send(info, ServerMessage::QuestsAssigned(SQuestsAssigned { quests }));
}

// Any change to the logged-in set can finish an `everyone` quest whose last
// holdout is gone.
pub fn recheck_everyone_quests(players: &mut PlayerMap, board: &mut QuestBoard, config: &ServerGameplayConfig) {
    for quest in &config.quests {
        if quest.scope != QuestScope::Everyone || !board.is_unlocked(&quest.id) || board.is_completed(&quest.id) {
            continue;
        }
        if everyone_count(players, quest).all_done() {
            complete_group(players, board, config, quest);
        }
    }
}

fn quest_points(config: &ServerGameplayConfig, quest: &Quest) -> i32 {
    config
        .scoring
        .quest_completed
        .get(&quest.id)
        .copied()
        .expect("quest id missing from scoring.quest_completed")
}

fn progress_message(quest: &Quest, progress: u32) -> ServerMessage {
    ServerMessage::QuestProgress(SQuestProgress {
        id: quest.id.clone(),
        progress,
    })
}

fn completed_message(quest: &Quest) -> ServerMessage {
    ServerMessage::QuestCompleted(SQuestCompleted {
        id: quest.id.clone(),
        completed_text: quest.completed_text.clone(),
    })
}

fn send(info: &PlayerInfo, message: ServerMessage) {
    let _ = info.channel.send(ServerToClient::Send(message));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quests::test_support::{
        assigned_ids, assignment_for, catalog, completed, drain, feed_events, join, join_with, own_progress,
        progress_values, quest, score,
    };

    fn cookie(players: &mut PlayerMap, board: &mut QuestBoard, config: &ServerGameplayConfig, id: u32) {
        record_player_event(players, board, config, PlayerId(id), PlayerQuestEvent::CookieCollected);
    }

    fn kill(players: &mut PlayerMap, board: &mut QuestBoard, config: &ServerGameplayConfig, id: u32, kind: &str) {
        record_player_event(
            players,
            board,
            config,
            PlayerId(id),
            PlayerQuestEvent::ActorKilled { kind },
        );
    }

    fn id(quest: &str) -> QuestId {
        QuestId(quest.to_owned())
    }

    #[test]
    fn individual_progress_and_completion_stay_per_player() {
        let config = catalog(vec![quest("gold", QuestKind::Cookies, QuestScope::Individual, 2, None)]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &config, &board);
        let mut bob = join(&mut players, 2, &config, &board);

        cookie(&mut players, &mut board, &config, 1);
        cookie(&mut players, &mut board, &config, 1);
        cookie(&mut players, &mut board, &config, 1);

        let alice_messages = drain(&mut alice);
        assert_eq!(
            progress_values(&alice_messages, "gold"),
            [1, 2],
            "the third cookie is past the threshold"
        );
        assert!(completed(&alice_messages, "gold"));
        assert_eq!(score(&players, 1), 100);
        let bob_messages = drain(&mut bob);
        assert!(!completed(&bob_messages, "gold"));
        assert!(
            matches!(feed_events(&bob_messages).as_slice(), [FeedEvent::QuestCompleted { name, .. }] if name == "P1")
        );
        assert_eq!(own_progress(&players, 2, "gold"), 0);
        assert_eq!(score(&players, 2), 0);
    }

    #[test]
    fn actor_kill_respects_kind_filter() {
        let mut sentries = quest("sentries", QuestKind::ActorKills, QuestScope::Individual, 2, None);
        sentries.actor_kind = Some("sentry".to_owned());
        let config = catalog(vec![sentries]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &config, &board);

        kill(&mut players, &mut board, &config, 1, "zapper");
        assert_eq!(own_progress(&players, 1, "sentries"), 0);
        kill(&mut players, &mut board, &config, 1, "sentry");
        assert_eq!(own_progress(&players, 1, "sentries"), 1);
    }

    #[test]
    fn shared_quest_pools_progress_and_scores_everyone_once() {
        let config = catalog(vec![quest("hunt", QuestKind::ActorKills, QuestScope::Shared, 2, None)]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &config, &board);
        let mut bob = join(&mut players, 2, &config, &board);

        kill(&mut players, &mut board, &config, 1, "sentry");
        assert_eq!(board.shared_progress(&id("hunt")), 1);
        assert!(!board.is_completed(&id("hunt")));

        kill(&mut players, &mut board, &config, 2, "sentry");
        assert!(board.is_completed(&id("hunt")));
        for rx in [&mut alice, &mut bob] {
            let messages = drain(rx);
            assert!(completed(&messages, "hunt"));
            assert!(
                progress_values(&messages, "hunt").is_empty(),
                "shared progress rides the snapshot only"
            );
            assert!(matches!(
                feed_events(&messages).as_slice(),
                [FeedEvent::GroupQuestCompleted { .. }]
            ));
        }
        assert_eq!((score(&players, 1), score(&players, 2)), (100, 100));

        kill(&mut players, &mut board, &config, 1, "sentry");
        assert!(drain(&mut alice).is_empty(), "latched: nothing after completion");
        assert_eq!(score(&players, 1), 100);
    }

    #[test]
    fn shared_quest_counts_events_from_a_departed_player() {
        let config = catalog(vec![quest("hunt", QuestKind::ActorKills, QuestScope::Shared, 2, None)]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &config, &board);
        let _bob = join(&mut players, 2, &config, &board);
        players.remove(&PlayerId(1));

        kill(&mut players, &mut board, &config, 1, "sentry");

        assert_eq!(
            board.shared_progress(&id("hunt")),
            1,
            "the pool doesn't care who is still here"
        );
    }

    #[test]
    fn everyone_quest_completes_when_the_last_player_reaches_the_threshold() {
        let config = catalog(vec![quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None)]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &config, &board);
        let mut bob = join(&mut players, 2, &config, &board);

        cookie(&mut players, &mut board, &config, 1);
        let alice_messages = drain(&mut alice);
        assert_eq!(progress_values(&alice_messages, "gold"), [1]);
        assert!(!completed(&alice_messages, "gold"));
        assert!(matches!(
            feed_events(&drain(&mut bob)).as_slice(),
            [FeedEvent::EveryoneQuestPartDone {
                players_done: 1,
                players_total: 2,
                ..
            }]
        ));

        cookie(&mut players, &mut board, &config, 2);
        for rx in [&mut alice, &mut bob] {
            let messages = drain(rx);
            assert!(completed(&messages, "gold"));
            let lines = feed_events(&messages);
            assert!(
                matches!(lines.as_slice(), [FeedEvent::GroupQuestCompleted { .. }]),
                "the last finisher gets no part-done line: {lines:?}"
            );
        }
        assert!(board.is_completed(&id("gold")));
        assert_eq!((score(&players, 1), score(&players, 2)), (100, 100));
    }

    #[test]
    fn everyone_quest_waits_for_a_dead_holdout() {
        let config = catalog(vec![quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None)]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &config, &board);
        let _ghost = join_with(&mut players, 2, &config, &board, true);

        cookie(&mut players, &mut board, &config, 1);
        assert!(
            !board.is_completed(&id("gold")),
            "a dead player still counts toward everyone"
        );

        cookie(&mut players, &mut board, &config, 2);
        assert!(board.is_completed(&id("gold")));
    }

    #[test]
    fn late_joiner_raises_the_denominator() {
        let config = catalog(vec![quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None)]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &config, &board);
        let _bob = join(&mut players, 2, &config, &board);
        cookie(&mut players, &mut board, &config, 1);
        drain(&mut alice);

        let _carol = join(&mut players, 3, &config, &board);
        cookie(&mut players, &mut board, &config, 2);
        assert!(!board.is_completed(&id("gold")));
        assert!(matches!(
            feed_events(&drain(&mut alice)).as_slice(),
            [FeedEvent::EveryoneQuestPartDone {
                players_done: 2,
                players_total: 3,
                ..
            }]
        ));

        cookie(&mut players, &mut board, &config, 3);
        assert!(board.is_completed(&id("gold")));
    }

    #[test]
    fn recheck_completes_when_the_leaver_was_the_holdout() {
        let config = catalog(vec![quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None)]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &config, &board);
        let _bob = join(&mut players, 2, &config, &board);
        cookie(&mut players, &mut board, &config, 1);
        drain(&mut alice);

        players.remove(&PlayerId(2));
        recheck_everyone_quests(&mut players, &mut board, &config);

        assert!(board.is_completed(&id("gold")));
        assert!(completed(&drain(&mut alice), "gold"));
        assert_eq!(score(&players, 1), 100);
    }

    #[test]
    fn recheck_completes_nothing_when_the_only_finisher_left() {
        let config = catalog(vec![quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None)]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &config, &board);
        let _bob = join(&mut players, 2, &config, &board);
        cookie(&mut players, &mut board, &config, 1);

        players.remove(&PlayerId(1));
        recheck_everyone_quests(&mut players, &mut board, &config);

        assert!(!board.is_completed(&id("gold")));
    }

    #[test]
    fn recheck_on_an_empty_server_completes_nothing() {
        let config = catalog(vec![quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None)]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &config, &board);
        players.remove(&PlayerId(1));

        recheck_everyone_quests(&mut players, &mut board, &config);

        assert!(!board.is_completed(&id("gold")));
    }

    #[test]
    fn group_completion_unlocks_dependents_and_assigns_them_to_everyone() {
        let config = catalog(vec![
            quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None),
            quest("show", QuestKind::Fireworks, QuestScope::Shared, 1, Some("gold")),
        ]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &config, &board);
        let mut bob = join(&mut players, 2, &config, &board);
        assert!(
            !players
                .get(&PlayerId(1))
                .expect("alice")
                .quest_states
                .contains_key(&id("show"))
        );

        cookie(&mut players, &mut board, &config, 1);
        cookie(&mut players, &mut board, &config, 2);

        assert!(board.is_unlocked(&id("show")));
        for rx in [&mut alice, &mut bob] {
            assert_eq!(assigned_ids(&drain(rx)), ["show"]);
        }
        assert!(
            players
                .get(&PlayerId(1))
                .expect("alice")
                .quest_states
                .contains_key(&id("show"))
        );

        // A late joiner gets the completed prerequisite as completed and the
        // unlocked quest fresh — and no points.
        let assigned = assignment_for(&config, &board);
        let ids: Vec<&str> = assigned.iter().map(|q| q.id.0.as_str()).collect();
        assert_eq!(ids, ["gold", "show"]);
        assert_eq!(assigned[0].progress, 1);
        assert_eq!(assigned[1].progress, 0);
        let _carol = join(&mut players, 3, &config, &board);
        assert_eq!(score(&players, 3), 0);
    }

    #[test]
    fn requires_chain_unlocks_one_step_at_a_time() {
        let config = catalog(vec![
            quest("gold", QuestKind::Cookies, QuestScope::Everyone, 1, None),
            quest("bonus", QuestKind::Cookies, QuestScope::Shared, 1, Some("gold")),
            quest("later", QuestKind::Cookies, QuestScope::Shared, 1, Some("bonus")),
        ]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &config, &board);

        cookie(&mut players, &mut board, &config, 1);
        assert!(board.is_completed(&id("gold")));
        assert!(board.is_unlocked(&id("bonus")) && !board.is_unlocked(&id("later")));
        assert_eq!(
            board.shared_progress(&id("bonus")),
            0,
            "the unlocking event doesn't feed what it unlocked"
        );

        cookie(&mut players, &mut board, &config, 1);
        assert!(board.is_completed(&id("bonus")));
        assert!(board.is_unlocked(&id("later")) && !board.is_completed(&id("later")));

        cookie(&mut players, &mut board, &config, 1);
        assert!(board.is_completed(&id("later")));
    }

    #[test]
    fn world_event_only_hits_its_kind() {
        let config = catalog(vec![
            quest("show", QuestKind::Fireworks, QuestScope::Shared, 1, None),
            quest("gold", QuestKind::Cookies, QuestScope::Individual, 5, None),
        ]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &config, &board);

        record_world_event(&mut players, &mut board, &config, WorldQuestEvent::FireworksStarted);

        assert!(board.is_completed(&id("show")));
        assert_eq!(own_progress(&players, 1, "gold"), 0);
    }

    #[test]
    fn dead_but_logged_in_players_are_credited_at_group_completion() {
        let config = catalog(vec![quest("show", QuestKind::Fireworks, QuestScope::Shared, 1, None)]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let mut alice = join(&mut players, 1, &config, &board);
        let mut ghost = join_with(&mut players, 2, &config, &board, true);

        record_world_event(&mut players, &mut board, &config, WorldQuestEvent::FireworksStarted);

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
        let mut board = QuestBoard::from_quests(&config.quests);
        board.latch_completed(&id("gold"));
        board.unlock(&id("show"));

        let assigned = assignment_for(&config, &board);

        let ids: Vec<&str> = assigned.iter().map(|q| q.id.0.as_str()).collect();
        assert_eq!(ids, ["gold", "show"]);
        assert_eq!(
            assigned[0].progress, 3,
            "completed group quests arrive at the threshold"
        );
        assert_eq!(assigned[1].order, 1, "order is the catalog index");
    }

    #[test]
    fn joining_against_a_live_pooled_counter_sees_the_pool() {
        let config = catalog(vec![quest("pool", QuestKind::Cookies, QuestScope::Shared, 5, None)]);
        let mut board = QuestBoard::from_quests(&config.quests);
        let mut players = PlayerMap::default();
        let _alice = join(&mut players, 1, &config, &board);
        for _ in 0..3 {
            cookie(&mut players, &mut board, &config, 1);
        }

        let assigned = assignment_for(&config, &board);
        assert_eq!(assigned[0].progress, 3);
    }
}
