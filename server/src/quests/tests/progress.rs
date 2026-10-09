use super::{
    QuestEvent,
    test_support::{Quests, assigned_ids, completed, drain, feed_lines, progress_values, quest},
    unlock_quest,
};
use crate::config::QuestKind;
use common::protocol::{PlayerId, QuestId, QuestScope, QuestState, QuestStateProgress};

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

fn everyone_gold() -> Quests {
    Quests::new(vec![quest("gold", QuestKind::Gold, QuestScope::Everyone, 1, None)])
}

#[test]
fn individual_progress_and_completion_stay_per_player() {
    let mut quests = Quests::new(vec![quest("gold", QuestKind::Gold, QuestScope::Individual, 2, None)]);
    let mut alice = quests.join(1);
    let mut bob = quests.join(2);

    for _ in 0..3 {
        quests.gold(1);
    }

    let alice_messages = drain(&mut alice);
    assert_eq!(
        progress_values(&alice_messages, "gold"),
        [1],
        "the third gold pickup is past the threshold"
    );
    assert!(completed(&alice_messages, "gold"));
    assert_eq!(quests.score(1), 100);
    let bob_messages = drain(&mut bob);
    assert!(!completed(&bob_messages, "gold"));
    assert_eq!(feed_lines(&bob_messages), ["P1 completed gold"]);
    assert_eq!(quests.own_progress(2, "gold"), 0);
    assert_eq!(quests.score(2), 0);
}

#[test]
fn actor_kill_respects_kind_filter() {
    let mut bruisers = quest("bruisers", QuestKind::ActorKills, QuestScope::Individual, 2, None);
    bruisers.actor_kind = Some("bruiser".to_owned());
    let mut quests = Quests::new(vec![bruisers]);
    let _alice = quests.join(1);

    quests.kill(1, "zapper");
    assert_eq!(quests.own_progress(1, "bruisers"), 0);
    quests.kill(1, "bruiser");
    assert_eq!(quests.own_progress(1, "bruisers"), 1);
}

#[test]
fn shared_quest_pools_progress_and_scores_everyone_once() {
    let mut quests = Quests::new(vec![quest("hunt", QuestKind::ActorKills, QuestScope::Shared, 2, None)]);
    let mut alice = quests.join(1);
    let mut bob = quests.join(2);

    quests.kill(1, "bruiser");
    assert_eq!(quests.board.shared_progress(&id("hunt")), 1);
    assert!(!quests.board.is_completed(&id("hunt")));

    quests.kill(2, "bruiser");
    assert!(quests.board.is_completed(&id("hunt")));
    for rx in [&mut alice, &mut bob] {
        let messages = drain(rx);
        assert!(completed(&messages, "hunt"));
        assert_eq!(progress_values(&messages, "hunt"), [1]);
        assert_eq!(feed_lines(&messages), ["Everyone completed hunt"]);
    }
    assert_eq!((quests.score(1), quests.score(2)), (100, 100));

    quests.kill(1, "bruiser");
    assert!(drain(&mut alice).is_empty(), "latched: nothing after completion");
    assert_eq!(quests.score(1), 100);
}

#[test]
fn shared_quest_counts_events_from_a_departed_player() {
    let mut quests = Quests::new(vec![quest("hunt", QuestKind::ActorKills, QuestScope::Shared, 2, None)]);
    let _alice = quests.join(1);
    let _bob = quests.join(2);
    quests.players.disconnect(&PlayerId(1), 2.0);

    quests.kill(1, "bruiser");

    assert_eq!(
        quests.board.shared_progress(&id("hunt")),
        1,
        "the pool doesn't care who is still here"
    );
}

#[test]
fn everyone_quest_completes_when_the_last_player_reaches_the_threshold() {
    let mut quests = everyone_gold();
    let mut alice = quests.join(1);
    let mut bob = quests.join(2);

    quests.gold(1);
    let alice_messages = drain(&mut alice);
    assert_eq!(progress_values(&alice_messages, "gold"), [1]);
    assert!(!completed(&alice_messages, "gold"));
    assert_eq!(feed_lines(&drain(&mut bob)), ["P1 finished gold (1/2 players)"]);

    quests.gold(2);
    for rx in [&mut alice, &mut bob] {
        let messages = drain(rx);
        assert!(completed(&messages, "gold"));
        assert_eq!(feed_lines(&messages), ["Everyone completed gold"]);
    }
    assert!(quests.board.is_completed(&id("gold")));
    assert_eq!((quests.score(1), quests.score(2)), (100, 100));
}

#[test]
fn everyone_quest_waits_for_a_dead_holdout() {
    let mut quests = everyone_gold();
    let _alice = quests.join(1);
    let _ghost = quests.join_dead(2);

    quests.gold(1);
    assert!(
        !quests.board.is_completed(&id("gold")),
        "a dead player still counts toward everyone"
    );

    quests.gold(2);
    assert!(quests.board.is_completed(&id("gold")));
}

#[test]
fn late_joiner_raises_the_denominator() {
    let mut quests = everyone_gold();
    let mut alice = quests.join(1);
    let _bob = quests.join(2);
    quests.gold(1);
    drain(&mut alice);

    let _carol = quests.join(3);
    quests.gold(2);
    assert!(!quests.board.is_completed(&id("gold")));
    assert_eq!(feed_lines(&drain(&mut alice)), ["P2 finished gold (2/3 players)"]);

    quests.gold(3);
    assert!(quests.board.is_completed(&id("gold")));
}

#[test]
fn recheck_completes_when_the_leaver_was_the_holdout() {
    let mut quests = everyone_gold();
    let mut alice = quests.join(1);
    let _bob = quests.join(2);
    quests.gold(1);
    drain(&mut alice);

    quests.players.disconnect(&PlayerId(2), 2.0);
    quests.recheck();

    assert!(quests.board.is_completed(&id("gold")));
    assert!(completed(&drain(&mut alice), "gold"));
    assert_eq!(quests.score(1), 100);
}

#[test]
fn recheck_completes_nothing_when_the_only_finisher_left() {
    let mut quests = everyone_gold();
    let _alice = quests.join(1);
    let _bob = quests.join(2);
    quests.gold(1);

    quests.players.disconnect(&PlayerId(1), 2.0);
    quests.recheck();

    assert!(!quests.board.is_completed(&id("gold")));
}

#[test]
fn recheck_on_an_empty_server_completes_nothing() {
    let mut quests = everyone_gold();
    let _alice = quests.join(1);
    quests.players.disconnect(&PlayerId(1), 2.0);

    quests.recheck();

    assert!(!quests.board.is_completed(&id("gold")));
}

#[test]
fn group_completion_unlocks_dependents_and_assigns_them_to_everyone() {
    let mut quests = Quests::new(vec![
        quest("gold", QuestKind::Gold, QuestScope::Everyone, 1, None),
        quest("show", QuestKind::Fireworks, QuestScope::Shared, 1, Some("gold")),
    ]);
    let mut alice = quests.join(1);
    let mut bob = quests.join(2);
    let assigned_show = |quests: &Quests| {
        quests
            .players
            .get(&PlayerId(1))
            .expect("alice")
            .session
            .quest_states
            .contains_key(&id("show"))
    };
    assert!(!assigned_show(&quests));

    quests.gold(1);
    quests.gold(2);

    assert!(quests.board.is_unlocked(&id("show")));
    for rx in [&mut alice, &mut bob] {
        assert_eq!(assigned_ids(&drain(rx)), ["show"]);
    }
    assert!(assigned_show(&quests));

    // A late joiner gets the completed prerequisite as completed and the
    // unlocked quest fresh — and no points.
    let assigned = quests.assignment();
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
    let _carol = quests.join(3);
    assert_eq!(quests.score(3), 0);
}

#[test]
fn requires_chain_unlocks_one_step_at_a_time() {
    let mut quests = Quests::new(vec![
        quest("gold", QuestKind::Gold, QuestScope::Everyone, 1, None),
        quest("bonus", QuestKind::Gold, QuestScope::Shared, 1, Some("gold")),
        quest("later", QuestKind::Gold, QuestScope::Shared, 1, Some("bonus")),
    ]);
    let _alice = quests.join(1);

    quests.gold(1);
    assert!(quests.board.is_completed(&id("gold")));
    assert!(quests.board.is_unlocked(&id("bonus")) && !quests.board.is_unlocked(&id("later")));
    assert_eq!(
        quests.board.shared_progress(&id("bonus")),
        0,
        "the unlocking event doesn't feed what it unlocked"
    );

    quests.gold(1);
    assert!(quests.board.is_completed(&id("bonus")));
    assert!(quests.board.is_unlocked(&id("later")) && !quests.board.is_completed(&id("later")));

    quests.gold(1);
    assert!(quests.board.is_completed(&id("later")));
}

#[test]
fn world_event_only_hits_its_kind() {
    let mut quests = Quests::new(vec![
        quest("show", QuestKind::Fireworks, QuestScope::Shared, 1, None),
        quest("gold", QuestKind::Gold, QuestScope::Individual, 5, None),
    ]);
    let _alice = quests.join(1);

    quests.record(QuestEvent::FireworksStarted);

    assert!(quests.board.is_completed(&id("show")));
    assert_eq!(quests.own_progress(1, "gold"), 0);
}

#[test]
fn dead_but_logged_in_players_are_credited_at_group_completion() {
    let mut quests = Quests::new(vec![quest("show", QuestKind::Fireworks, QuestScope::Shared, 1, None)]);
    let mut alice = quests.join(1);
    let mut ghost = quests.join_dead(2);

    quests.record(QuestEvent::FireworksStarted);

    for rx in [&mut alice, &mut ghost] {
        assert!(completed(&drain(rx), "show"));
    }
    assert_eq!((quests.score(1), quests.score(2)), (100, 100));
}

#[test]
fn assign_quests_skips_locked_and_seeds_completed_group_quests() {
    let mut quests = Quests::new(vec![
        quest("gold", QuestKind::Gold, QuestScope::Everyone, 3, None),
        quest("show", QuestKind::Fireworks, QuestScope::Shared, 1, Some("gold")),
        quest("later", QuestKind::Gold, QuestScope::Shared, 1, Some("show")),
    ]);
    quests
        .board
        .finish_group(quests.catalog.get(&id("gold")).expect("gold quest missing"));
    quests.board.unlock(&id("show"));

    let assigned = quests.assignment();

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
    let mut quests = Quests::new(vec![quest("pool", QuestKind::Gold, QuestScope::Shared, 5, None)]);
    let _alice = quests.join(1);
    for _ in 0..3 {
        quests.gold(1);
    }

    let assigned = quests.assignment();
    assert_eq!(initial_progress(&assigned[0]), 3);
}

#[test]
fn admin_completion_finishes_individual_quests_for_the_targets_only() {
    let mut quests = Quests::new(vec![quest("gold", QuestKind::Gold, QuestScope::Individual, 3, None)]);
    let mut alice = quests.join(1);
    let _bob = quests.join(2);

    assert_eq!(quests.complete("gold", &[PlayerId(1)]), 1);

    let messages = drain(&mut alice);
    assert!(progress_values(&messages, "gold").is_empty());
    assert!(completed(&messages, "gold"));
    assert_eq!(quests.score(1), 100);
    assert_eq!(quests.own_progress(2, "gold"), 0);
    assert_eq!(quests.complete("gold", &[PlayerId(1)]), 0, "already finished");
}

#[test]
fn admin_completion_of_an_everyone_quest_completes_the_group_once_every_part_is_done() {
    let mut quests = Quests::new(vec![quest("gold", QuestKind::Gold, QuestScope::Everyone, 3, None)]);
    let mut alice = quests.join(1);
    let _bob = quests.join(2);

    assert_eq!(quests.complete("gold", &[PlayerId(1)]), 1);
    assert!(!quests.board.is_completed(&id("gold")));
    assert_eq!(feed_lines(&drain(&mut alice)), ["P1 finished gold (1/2 players)"]);

    assert_eq!(
        quests.complete("gold", &[PlayerId(1), PlayerId(2)]),
        1,
        "only Bob's part was still open"
    );
    assert!(quests.board.is_completed(&id("gold")));
    assert_eq!(quests.score(1), 100);
    assert_eq!(quests.score(2), 100);
}

#[test]
fn admin_unlock_and_shared_completion_bypass_the_prerequisite() {
    let mut quests = Quests::new(vec![
        quest("hunt", QuestKind::ActorKills, QuestScope::Shared, 5, None),
        quest("bonus", QuestKind::Gold, QuestScope::Individual, 1, Some("hunt")),
    ]);
    let mut alice = quests.join(1);

    unlock_quest(&mut quests.players, &mut quests.board, &quests.catalog, &id("bonus"));
    assert!(quests.board.is_unlocked(&id("bonus")));
    assert_eq!(assigned_ids(&drain(&mut alice)), ["bonus"]);

    assert_eq!(quests.complete("hunt", &[]), 0, "a shared quest has no own parts");
    assert!(quests.board.is_completed(&id("hunt")));
    assert_eq!(quests.board.shared_progress(&id("hunt")), 5);
    assert!(completed(&drain(&mut alice), "hunt"));
    assert_eq!(quests.score(1), 100);

    quests.complete("hunt", &[]);
    assert_eq!(quests.score(1), 100);
    assert!(drain(&mut alice).is_empty());
}
