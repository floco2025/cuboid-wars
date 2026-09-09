use super::{
    QuestBoard, QuestCatalog,
    progress::*,
    test_support::{
        assigned_ids, assignment_for, catalog, completed, drain, feed_lines, join, join_with, own_progress,
        progress_values, quest, score,
    },
};
use crate::{
    config::{FeedConfig, QuestKind},
    players::PlayerMap,
};
use common::protocol::{PlayerId, QuestId, QuestScope, QuestState, QuestStateProgress};

fn gold(players: &mut PlayerMap, board: &mut QuestBoard, catalog: &QuestCatalog, feed: &FeedConfig, id: u32) {
    record_event(
        players,
        board,
        catalog,
        feed,
        QuestEvent::GoldCollected { player: PlayerId(id) },
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
    let config = catalog(vec![quest("gold", QuestKind::Gold, QuestScope::Individual, 2, None)]);
    let quest_catalog = QuestCatalog::from_config(&config);
    let mut board = QuestBoard::from_catalog(&quest_catalog);
    let mut players = PlayerMap::default();
    let mut alice = join(&mut players, 1, &quest_catalog, &board);
    let mut bob = join(&mut players, 2, &quest_catalog, &board);

    gold(&mut players, &mut board, &quest_catalog, &config.feed, 1);
    gold(&mut players, &mut board, &quest_catalog, &config.feed, 1);
    gold(&mut players, &mut board, &quest_catalog, &config.feed, 1);

    let alice_messages = drain(&mut alice);
    assert_eq!(
        progress_values(&alice_messages, "gold"),
        [1],
        "the third gold pickup is past the threshold"
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
    let mut bruisers = quest("bruisers", QuestKind::ActorKills, QuestScope::Individual, 2, None);
    bruisers.actor_kind = Some("bruiser".to_owned());
    let config = catalog(vec![bruisers]);
    let quest_catalog = QuestCatalog::from_config(&config);
    let mut board = QuestBoard::from_catalog(&quest_catalog);
    let mut players = PlayerMap::default();
    let _alice = join(&mut players, 1, &quest_catalog, &board);

    kill(&mut players, &mut board, &quest_catalog, &config.feed, 1, "zapper");
    assert_eq!(own_progress(&players, 1, "bruisers"), 0);
    kill(&mut players, &mut board, &quest_catalog, &config.feed, 1, "bruiser");
    assert_eq!(own_progress(&players, 1, "bruisers"), 1);
}

#[test]
fn shared_quest_pools_progress_and_scores_everyone_once() {
    let config = catalog(vec![quest("hunt", QuestKind::ActorKills, QuestScope::Shared, 2, None)]);
    let quest_catalog = QuestCatalog::from_config(&config);
    let mut board = QuestBoard::from_catalog(&quest_catalog);
    let mut players = PlayerMap::default();
    let mut alice = join(&mut players, 1, &quest_catalog, &board);
    let mut bob = join(&mut players, 2, &quest_catalog, &board);

    kill(&mut players, &mut board, &quest_catalog, &config.feed, 1, "bruiser");
    assert_eq!(board.shared_progress(&id("hunt")), 1);
    assert!(!board.is_completed(&id("hunt")));

    kill(&mut players, &mut board, &quest_catalog, &config.feed, 2, "bruiser");
    assert!(board.is_completed(&id("hunt")));
    for rx in [&mut alice, &mut bob] {
        let messages = drain(rx);
        assert!(completed(&messages, "hunt"));
        assert_eq!(progress_values(&messages, "hunt"), [1]);
        assert_eq!(feed_lines(&messages), ["Everyone completed hunt"]);
    }
    assert_eq!((score(&players, 1), score(&players, 2)), (100, 100));

    kill(&mut players, &mut board, &quest_catalog, &config.feed, 1, "bruiser");
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
    players.disconnect(&PlayerId(1), 2.0);

    kill(&mut players, &mut board, &quest_catalog, &config.feed, 1, "bruiser");

    assert_eq!(
        board.shared_progress(&id("hunt")),
        1,
        "the pool doesn't care who is still here"
    );
}

#[test]
fn everyone_quest_completes_when_the_last_player_reaches_the_threshold() {
    let config = catalog(vec![quest("gold", QuestKind::Gold, QuestScope::Everyone, 1, None)]);
    let quest_catalog = QuestCatalog::from_config(&config);
    let mut board = QuestBoard::from_catalog(&quest_catalog);
    let mut players = PlayerMap::default();
    let mut alice = join(&mut players, 1, &quest_catalog, &board);
    let mut bob = join(&mut players, 2, &quest_catalog, &board);

    gold(&mut players, &mut board, &quest_catalog, &config.feed, 1);
    let alice_messages = drain(&mut alice);
    assert_eq!(progress_values(&alice_messages, "gold"), [1]);
    assert!(!completed(&alice_messages, "gold"));
    assert_eq!(feed_lines(&drain(&mut bob)), ["P1 finished gold (1/2 players)"]);

    gold(&mut players, &mut board, &quest_catalog, &config.feed, 2);
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
    let config = catalog(vec![quest("gold", QuestKind::Gold, QuestScope::Everyone, 1, None)]);
    let quest_catalog = QuestCatalog::from_config(&config);
    let mut board = QuestBoard::from_catalog(&quest_catalog);
    let mut players = PlayerMap::default();
    let _alice = join(&mut players, 1, &quest_catalog, &board);
    let _ghost = join_with(&mut players, 2, &quest_catalog, &board, true);

    gold(&mut players, &mut board, &quest_catalog, &config.feed, 1);
    assert!(
        !board.is_completed(&id("gold")),
        "a dead player still counts toward everyone"
    );

    gold(&mut players, &mut board, &quest_catalog, &config.feed, 2);
    assert!(board.is_completed(&id("gold")));
}

#[test]
fn late_joiner_raises_the_denominator() {
    let config = catalog(vec![quest("gold", QuestKind::Gold, QuestScope::Everyone, 1, None)]);
    let quest_catalog = QuestCatalog::from_config(&config);
    let mut board = QuestBoard::from_catalog(&quest_catalog);
    let mut players = PlayerMap::default();
    let mut alice = join(&mut players, 1, &quest_catalog, &board);
    let _bob = join(&mut players, 2, &quest_catalog, &board);
    gold(&mut players, &mut board, &quest_catalog, &config.feed, 1);
    drain(&mut alice);

    let _carol = join(&mut players, 3, &quest_catalog, &board);
    gold(&mut players, &mut board, &quest_catalog, &config.feed, 2);
    assert!(!board.is_completed(&id("gold")));
    assert_eq!(feed_lines(&drain(&mut alice)), ["P2 finished gold (2/3 players)"]);

    gold(&mut players, &mut board, &quest_catalog, &config.feed, 3);
    assert!(board.is_completed(&id("gold")));
}

#[test]
fn recheck_completes_when_the_leaver_was_the_holdout() {
    let config = catalog(vec![quest("gold", QuestKind::Gold, QuestScope::Everyone, 1, None)]);
    let quest_catalog = QuestCatalog::from_config(&config);
    let mut board = QuestBoard::from_catalog(&quest_catalog);
    let mut players = PlayerMap::default();
    let mut alice = join(&mut players, 1, &quest_catalog, &board);
    let _bob = join(&mut players, 2, &quest_catalog, &board);
    gold(&mut players, &mut board, &quest_catalog, &config.feed, 1);
    drain(&mut alice);

    players.disconnect(&PlayerId(2), 2.0);
    recheck_everyone_quests(&mut players, &mut board, &quest_catalog, &config.feed);

    assert!(board.is_completed(&id("gold")));
    assert!(completed(&drain(&mut alice), "gold"));
    assert_eq!(score(&players, 1), 100);
}

#[test]
fn recheck_completes_nothing_when_the_only_finisher_left() {
    let config = catalog(vec![quest("gold", QuestKind::Gold, QuestScope::Everyone, 1, None)]);
    let quest_catalog = QuestCatalog::from_config(&config);
    let mut board = QuestBoard::from_catalog(&quest_catalog);
    let mut players = PlayerMap::default();
    let _alice = join(&mut players, 1, &quest_catalog, &board);
    let _bob = join(&mut players, 2, &quest_catalog, &board);
    gold(&mut players, &mut board, &quest_catalog, &config.feed, 1);

    players.disconnect(&PlayerId(1), 2.0);
    recheck_everyone_quests(&mut players, &mut board, &quest_catalog, &config.feed);

    assert!(!board.is_completed(&id("gold")));
}

#[test]
fn recheck_on_an_empty_server_completes_nothing() {
    let config = catalog(vec![quest("gold", QuestKind::Gold, QuestScope::Everyone, 1, None)]);
    let quest_catalog = QuestCatalog::from_config(&config);
    let mut board = QuestBoard::from_catalog(&quest_catalog);
    let mut players = PlayerMap::default();
    let _alice = join(&mut players, 1, &quest_catalog, &board);
    players.disconnect(&PlayerId(1), 2.0);

    recheck_everyone_quests(&mut players, &mut board, &quest_catalog, &config.feed);

    assert!(!board.is_completed(&id("gold")));
}

#[test]
fn group_completion_unlocks_dependents_and_assigns_them_to_everyone() {
    let config = catalog(vec![
        quest("gold", QuestKind::Gold, QuestScope::Everyone, 1, None),
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

    gold(&mut players, &mut board, &quest_catalog, &config.feed, 1);
    gold(&mut players, &mut board, &quest_catalog, &config.feed, 2);

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
        quest("gold", QuestKind::Gold, QuestScope::Everyone, 1, None),
        quest("bonus", QuestKind::Gold, QuestScope::Shared, 1, Some("gold")),
        quest("later", QuestKind::Gold, QuestScope::Shared, 1, Some("bonus")),
    ]);
    let quest_catalog = QuestCatalog::from_config(&config);
    let mut board = QuestBoard::from_catalog(&quest_catalog);
    let mut players = PlayerMap::default();
    let _alice = join(&mut players, 1, &quest_catalog, &board);

    gold(&mut players, &mut board, &quest_catalog, &config.feed, 1);
    assert!(board.is_completed(&id("gold")));
    assert!(board.is_unlocked(&id("bonus")) && !board.is_unlocked(&id("later")));
    assert_eq!(
        board.shared_progress(&id("bonus")),
        0,
        "the unlocking event doesn't feed what it unlocked"
    );

    gold(&mut players, &mut board, &quest_catalog, &config.feed, 1);
    assert!(board.is_completed(&id("bonus")));
    assert!(board.is_unlocked(&id("later")) && !board.is_completed(&id("later")));

    gold(&mut players, &mut board, &quest_catalog, &config.feed, 1);
    assert!(board.is_completed(&id("later")));
}

#[test]
fn world_event_only_hits_its_kind() {
    let config = catalog(vec![
        quest("show", QuestKind::Fireworks, QuestScope::Shared, 1, None),
        quest("gold", QuestKind::Gold, QuestScope::Individual, 5, None),
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
        quest("gold", QuestKind::Gold, QuestScope::Everyone, 3, None),
        quest("show", QuestKind::Fireworks, QuestScope::Shared, 1, Some("gold")),
        quest("later", QuestKind::Gold, QuestScope::Shared, 1, Some("show")),
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
    let config = catalog(vec![quest("pool", QuestKind::Gold, QuestScope::Shared, 5, None)]);
    let quest_catalog = QuestCatalog::from_config(&config);
    let mut board = QuestBoard::from_catalog(&quest_catalog);
    let mut players = PlayerMap::default();
    let _alice = join(&mut players, 1, &quest_catalog, &board);
    for _ in 0..3 {
        gold(&mut players, &mut board, &quest_catalog, &config.feed, 1);
    }

    let assigned = assignment_for(&quest_catalog, &board);
    assert_eq!(initial_progress(&assigned[0]), 3);
}

#[test]
fn admin_completion_finishes_individual_quests_for_the_targets_only() {
    let config = catalog(vec![quest("gold", QuestKind::Gold, QuestScope::Individual, 3, None)]);
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
    let config = catalog(vec![quest("gold", QuestKind::Gold, QuestScope::Everyone, 3, None)]);
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
        quest("bonus", QuestKind::Gold, QuestScope::Individual, 1, Some("hunt")),
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
