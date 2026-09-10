use super::*;
use crate::ui::QuestProgress;

fn state(id: &str, scope: QuestScope, progress: u32, completed: bool, order: u32) -> QuestState {
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
        id: QuestId(id.to_owned()),
        title: id.to_uppercase(),
        description: format!("do {id}"),
        completed_text: format!("{id} done!"),
        threshold: 10,
        scope,
        order,
        status: QuestStatus { completed, progress },
    }
}

fn update(reason: QuestUpdateReason, quest: QuestState) -> QuestUpdate {
    QuestUpdate { reason, quest }
}

fn apply(log: &mut QuestLog, banner: &mut HudBanner, updates: Vec<QuestUpdate>) -> bool {
    apply_quest_updates(log, banner, SQuestUpdates { updates })
}

#[test]
fn assigned_batch_announces_new_quests_once() {
    let mut log = QuestLog::default();
    let mut banner = HudBanner::default();
    let updates = vec![
        update(
            QuestUpdateReason::Assigned,
            state("gold", QuestScope::Individual, 0, false, 0),
        ),
        update(
            QuestUpdateReason::Assigned,
            state("hunt", QuestScope::Shared, 0, false, 1),
        ),
    ];

    assert!(!apply(&mut log, &mut banner, updates.clone()));
    assert_eq!(banner.pending_texts(), ["GOLD: do gold\nHUNT: do hunt"]);

    assert!(!apply(&mut log, &mut banner, updates));
    assert_eq!(banner.pending_texts().len(), 1, "known quests announce nothing");
}

#[test]
fn assigned_finished_quest_is_installed_silently() {
    let mut log = QuestLog::default();
    let mut banner = HudBanner::default();
    let updates = vec![
        update(
            QuestUpdateReason::Assigned,
            state("gold", QuestScope::Everyone, 10, true, 0),
        ),
        update(
            QuestUpdateReason::Assigned,
            state("show", QuestScope::Shared, 0, false, 1),
        ),
    ];

    assert!(!apply(&mut log, &mut banner, updates));

    assert!(log.entry("gold").expect("gold quest missing").completed);
    assert_eq!(banner.pending_texts(), ["SHOW: do show"]);
}

#[test]
fn complete_updates_are_independent_and_stale_updates_do_not_regress() {
    let mut log = QuestLog::default();
    let mut banner = HudBanner::default();

    assert!(apply(
        &mut log,
        &mut banner,
        vec![update(
            QuestUpdateReason::Completed,
            state("hunt", QuestScope::Shared, 10, true, 0),
        )],
    ));
    assert_eq!(banner.pending_texts(), ["hunt done!"]);

    assert!(!apply(
        &mut log,
        &mut banner,
        vec![update(
            QuestUpdateReason::Progressed,
            state("hunt", QuestScope::Shared, 4, false, 0),
        )],
    ));
    let hunt = log.entry("hunt").expect("hunt quest missing");
    assert!(hunt.completed);
    assert_eq!(hunt.progress, QuestProgress::Shared(10));
}

#[test]
fn progressed_update_can_install_a_missing_quest() {
    let mut log = QuestLog::default();
    let mut banner = HudBanner::default();

    assert!(!apply(
        &mut log,
        &mut banner,
        vec![update(
            QuestUpdateReason::Progressed,
            state("gold", QuestScope::Everyone, 3, false, 0),
        )],
    ));

    assert_eq!(banner.pending_texts(), ["GOLD: do gold"]);
    assert_eq!(
        log.entry("gold").expect("gold quest missing").progress,
        QuestProgress::Everyone {
            own: 3,
            players_done: 0,
            players_total: 0,
        }
    );
}
