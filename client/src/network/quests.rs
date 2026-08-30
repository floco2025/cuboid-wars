use bevy::prelude::*;
use common::protocol::*;

use super::context::ServerMessageContext;
use crate::{
    audio::play_sound,
    ui::{BannerMessage, HudBanner, QuestEntry, QuestLog, QuestProgress},
};

pub(super) fn handle_quests_assigned_message(message: SQuestsAssigned, context: &mut ServerMessageContext) {
    apply_quests_assigned(&mut context.quest_log, &mut context.banner, message);
}

pub(super) fn handle_quest_progress_message(message: SQuestProgress, context: &mut ServerMessageContext) {
    apply_quest_progress(&mut context.quest_log, message);
}

pub(super) fn handle_quest_completed_message(
    message: SQuestCompleted,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    apply_quest_completed(&mut context.quest_log, &mut context.banner, message);
    play_sound(
        commands,
        &context.asset_server,
        context.asset_set.player_sound("quest_completed"),
    );
}

// Assignments from one server batch share one banner instead of fading separately.
fn apply_quests_assigned(quest_log: &mut QuestLog, banner: &mut HudBanner, event: SQuestsAssigned) {
    let mut lines = Vec::new();
    for quest in event.quests {
        let entry = QuestEntry {
            title: quest.title,
            description: quest.description,
            threshold: quest.threshold,
            progress: QuestProgress::from_initial(quest.status.progress),
            completed: quest.status.completed,
            order: quest.order,
        };
        let announcement = entry.announcement();
        if quest_log.assign(quest.id, entry) {
            lines.push(announcement);
        }
    }
    if !lines.is_empty() {
        banner.push(BannerMessage::QuestAnnouncement(lines.join("\n")));
    }
}

fn apply_quest_progress(quest_log: &mut QuestLog, event: SQuestProgress) {
    quest_log.record_progress(event.id, event.progress);
}

fn apply_quest_completed(quest_log: &mut QuestLog, banner: &mut HudBanner, event: SQuestCompleted) {
    quest_log.record_completion(event.id);
    banner.push(BannerMessage::QuestCompleted(event.completed_text));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_quest(id: &str, scope: QuestScope, progress: u32, order: u32) -> NewQuest {
        let progress = match scope {
            QuestScope::Individual => QuestInitialProgress::Individual { progress },
            QuestScope::Shared => QuestInitialProgress::Shared { progress },
            QuestScope::Everyone => QuestInitialProgress::Everyone {
                progress,
                players_done: 0,
                players_total: 0,
            },
        };
        NewQuest {
            id: QuestId(id.to_owned()),
            title: id.to_uppercase(),
            description: format!("do {id}"),
            threshold: 10,
            status: QuestInitialStatus {
                completed: false,
                progress,
            },
            order,
        }
    }

    fn assigned(quests: Vec<NewQuest>) -> SQuestsAssigned {
        SQuestsAssigned { quests }
    }

    fn progress(id: &str, progress: u32) -> SQuestProgress {
        SQuestProgress {
            id: QuestId(id.to_owned()),
            progress,
        }
    }

    fn completed(id: &str) -> SQuestCompleted {
        SQuestCompleted {
            id: QuestId(id.to_owned()),
            completed_text: format!("{id} done!"),
        }
    }

    #[test]
    fn assignment_announces_new_quests_once_as_one_line() {
        let mut log = QuestLog::default();
        let mut banner = HudBanner::default();
        let event = assigned(vec![
            new_quest("gold", QuestScope::Individual, 0, 0),
            new_quest("hunt", QuestScope::Shared, 0, 1),
        ]);

        apply_quests_assigned(&mut log, &mut banner, event.clone());
        assert_eq!(banner.pending_texts(), ["GOLD: do gold\nHUNT: do hunt"]);

        apply_quests_assigned(&mut log, &mut banner, event);
        assert_eq!(banner.pending_texts().len(), 1, "known ids announce nothing");
    }

    #[test]
    fn events_before_assignment_land_on_the_entry() {
        let mut log = QuestLog::default();
        let mut banner = HudBanner::default();

        apply_quest_progress(&mut log, progress("gold", 9));
        apply_quest_completed(&mut log, &mut banner, completed("hunt"));
        assert_eq!(banner.pending_texts(), ["hunt done!"]);

        apply_quests_assigned(
            &mut log,
            &mut banner,
            assigned(vec![
                new_quest("gold", QuestScope::Individual, 2, 0),
                new_quest("hunt", QuestScope::Shared, 0, 1),
            ]),
        );

        assert_eq!(
            log.entry("gold").expect("gold quest missing").progress,
            QuestProgress::Own(9)
        );
        let hunt = log.entry("hunt").expect("hunt quest missing");
        assert!(hunt.completed);
        assert_eq!(hunt.progress, QuestProgress::Shared(10));
    }

    #[test]
    fn everyone_quests_start_with_their_own_counter() {
        let mut log = QuestLog::default();
        let mut banner = HudBanner::default();

        apply_quests_assigned(
            &mut log,
            &mut banner,
            assigned(vec![new_quest("gold", QuestScope::Everyone, 3, 0)]),
        );

        assert_eq!(
            log.entry("gold").expect("gold quest missing").progress,
            QuestProgress::Everyone {
                own: 3,
                players_done: 0,
                players_total: 0
            }
        );
    }
}
