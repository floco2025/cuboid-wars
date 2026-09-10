use super::{super::test_support::*, *};

fn id(id: &str) -> QuestId {
    QuestId(id.to_owned())
}

fn status(id: &str, completed: bool, progress: QuestGroupProgress) -> QuestGroupStatus {
    QuestGroupStatus {
        id: QuestId(id.to_owned()),
        completed,
        progress,
    }
}

fn everyone(players_done: u32, players_total: u32) -> QuestGroupProgress {
    QuestGroupProgress::Everyone {
        players_done,
        players_total,
    }
}

fn quest_state(quest_id: &str, scope: QuestScope, progress: u32, completed: bool) -> QuestState {
    let progress = match scope {
        QuestScope::Individual => QuestStateProgress::Individual { progress },
        QuestScope::Shared => QuestStateProgress::Shared { progress },
        QuestScope::Everyone => QuestStateProgress::Everyone {
            progress,
            players_done: 0,
            players_total: 0,
        },
    };
    QuestState {
        id: id(quest_id),
        title: quest_id.to_owned(),
        description: format!("do {quest_id}"),
        completed_text: format!("{quest_id} done"),
        threshold: 10,
        scope,
        order: 0,
        status: common::protocol::QuestStatus { completed, progress },
    }
}

#[test]
fn assign_rejects_a_known_id() {
    let mut log = log(vec![("gold", entry("Gold", QuestScope::Individual, 0, 10, 0))]);
    assert!(!log.assign(id("gold"), entry("Gold", QuestScope::Individual, 0, 10, 0)));
}

#[test]
fn progress_keeps_the_max() {
    let mut log = QuestLog::default();

    log.apply_state(quest_state("gold", QuestScope::Individual, 4, false))
        .expect("first state should apply");
    log.apply_state(quest_state("gold", QuestScope::Individual, 2, false))
        .expect("stale state should apply without regressing");

    assert_eq!(log.entry("gold").expect("assigned").progress, QuestProgress::Own(4));
}

#[test]
fn full_progress_state_installs_the_quest() {
    let mut log = QuestLog::default();
    let change = log
        .apply_state(quest_state("gold", QuestScope::Individual, 9, false))
        .expect("full state should apply");

    assert!(change.inserted);
    assert_eq!(log.entry("gold").expect("assigned").progress, QuestProgress::Own(9));
}

#[test]
fn stale_progress_after_completion_does_not_regress() {
    let mut log = QuestLog::default();
    log.apply_state(quest_state("gold", QuestScope::Individual, 10, true))
        .expect("completion should apply");
    log.apply_state(quest_state("gold", QuestScope::Individual, 3, false))
        .expect("stale progress should merge");

    let entry = log.entry("gold").expect("assigned");
    assert!(entry.completed);
    assert_eq!(entry.progress, QuestProgress::Own(10));
}

#[test]
fn group_status_sets_everyone_counts_and_can_lower_them() {
    let mut log = log(vec![("gold", entry("Gold", QuestScope::Everyone, 3, 10, 0))]);

    log.apply_group_status(&[status("gold", false, everyone(2, 3))]);
    log.apply_group_status(&[status("gold", false, everyone(1, 2))]);

    assert_eq!(
        log.entry("gold").expect("assigned").progress,
        QuestProgress::Everyone {
            own: 3,
            players_done: 1,
            players_total: 2
        },
        "counts are set, own progress untouched"
    );
}

#[test]
fn group_status_shared_progress_max_merges() {
    let mut log = log(vec![("hunt", entry("Hunt", QuestScope::Shared, 0, 4, 0))]);

    log.apply_group_status(&[status("hunt", false, QuestGroupProgress::Shared { progress: 3 })]);
    log.apply_group_status(&[status("hunt", false, QuestGroupProgress::Shared { progress: 2 })]);

    assert_eq!(log.entry("hunt").expect("assigned").progress, QuestProgress::Shared(3));
}

#[test]
fn group_status_before_quest_state_is_ignored_and_next_snapshot_repairs_it() {
    let mut log = QuestLog::default();
    log.apply_group_status(&[status("show", true, QuestGroupProgress::Shared { progress: 1 })]);
    assert!(log.sorted().is_empty());

    assert!(log.assign(id("show"), entry("Show", QuestScope::Shared, 0, 1, 0)));
    assert!(!log.entry("show").expect("assigned").completed);
    log.apply_group_status(&[status("show", true, QuestGroupProgress::Shared { progress: 1 })]);

    let entry = log.entry("show").expect("assigned");
    assert!(entry.completed);
    assert_eq!(entry.progress, QuestProgress::Shared(1));
}

#[test]
fn everyone_counts_before_quest_state_are_repaired_by_the_next_snapshot() {
    let mut log = QuestLog::default();
    log.apply_group_status(&[status("gold", false, everyone(2, 3))]);

    assert!(log.assign(id("gold"), entry("Gold", QuestScope::Everyone, 1, 10, 0)));
    log.apply_group_status(&[status("gold", false, everyone(2, 3))]);

    assert_eq!(
        log.entry("gold").expect("assigned").progress,
        QuestProgress::Everyone {
            own: 1,
            players_done: 2,
            players_total: 3,
        }
    );
}

#[test]
fn sorted_ranks_by_catalog_order_then_id() {
    // Ids sort "a" < "z", but catalog order puts "z" first; equal order
    // falls back to id.
    let log = log(vec![
        ("a_quest", entry("Second", QuestScope::Individual, 0, 1, 1)),
        ("z_quest", entry("First", QuestScope::Individual, 0, 1, 0)),
        ("b_tie", entry("Tie B", QuestScope::Individual, 0, 1, 5)),
        ("a_tie", entry("Tie A", QuestScope::Individual, 0, 1, 5)),
    ]);

    let ids: Vec<&str> = log.sorted().into_iter().map(|(id, _)| id.0.as_str()).collect();

    assert_eq!(ids, ["z_quest", "a_quest", "a_tie", "b_tie"]);
}

#[test]
fn reminder_lists_only_quests_still_to_do_in_order() {
    let mut log = log(vec![
        ("hunt", entry("Hunt", QuestScope::Shared, 1, 4, 1)),
        ("gold", entry("Gold", QuestScope::Everyone, 0, 10, 0)),
        ("done", entry("Done", QuestScope::Individual, 0, 1, 2)),
        ("part", entry("Part", QuestScope::Everyone, 10, 10, 3)),
    ]);
    log.record_completion(id("done"));

    assert_eq!(
        log.reminder().as_deref(),
        Some("Gold: Gold description\nHunt: Hunt description"),
        "completed quests and a finished own part are left out"
    );

    assert_eq!(QuestLog::default().reminder(), None);
}
