use common::protocol::*;

use crate::ui::{BannerMessage, HudBanner, QuestEntry, QuestLog, QuestProgress};

// Per-client quest state events. Both dispatchers route them here: they
// install durable state with no snapshot fallback and don't need
// `MyPlayerId`, so they are handled even before bootstrap. Anything else is
// handed back.
pub fn handle_quest_message(
    quest_log: &mut QuestLog,
    banner: &mut HudBanner,
    msg: ServerMessage,
) -> Option<ServerMessage> {
    match msg {
        ServerMessage::QuestsAssigned(msg) => handle_quests_assigned_message(quest_log, banner, msg),
        ServerMessage::QuestProgress(msg) => quest_log.record_progress(msg.id, msg.progress),
        ServerMessage::QuestCompleted(msg) => {
            quest_log.record_completion(msg.id);
            banner.push(BannerMessage::QuestCompleted(msg.completed_text));
        }
        other => return Some(other),
    }
    None
}

// ONE combined announcement for the batch — one band line, not N staggered
// fades. Already-known ids announce nothing.
fn handle_quests_assigned_message(quest_log: &mut QuestLog, banner: &mut HudBanner, msg: SQuestsAssigned) {
    let mut lines = Vec::new();
    for quest in msg.quests {
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

    fn assigned(quests: Vec<NewQuest>) -> ServerMessage {
        ServerMessage::QuestsAssigned(SQuestsAssigned { quests })
    }

    fn progress(id: &str, progress: u32) -> ServerMessage {
        ServerMessage::QuestProgress(SQuestProgress {
            id: QuestId(id.to_owned()),
            progress,
        })
    }

    fn completed(id: &str) -> ServerMessage {
        ServerMessage::QuestCompleted(SQuestCompleted {
            id: QuestId(id.to_owned()),
            completed_text: format!("{id} done!"),
        })
    }

    #[test]
    fn assignment_announces_new_quests_once_as_one_line() {
        let mut log = QuestLog::default();
        let mut banner = HudBanner::default();
        let msg = assigned(vec![
            new_quest("gold", QuestScope::Individual, 0, 0),
            new_quest("hunt", QuestScope::Shared, 0, 1),
        ]);

        assert!(handle_quest_message(&mut log, &mut banner, msg.clone()).is_none());
        assert_eq!(banner.pending_texts(), ["GOLD: do gold\nHUNT: do hunt"]);

        handle_quest_message(&mut log, &mut banner, msg);
        assert_eq!(banner.pending_texts().len(), 1, "known ids announce nothing");
    }

    #[test]
    fn events_before_assignment_land_on_the_entry() {
        let mut log = QuestLog::default();
        let mut banner = HudBanner::default();

        handle_quest_message(&mut log, &mut banner, progress("gold", 9));
        handle_quest_message(&mut log, &mut banner, completed("hunt"));
        assert_eq!(banner.pending_texts(), ["hunt done!"]);

        handle_quest_message(
            &mut log,
            &mut banner,
            assigned(vec![
                new_quest("gold", QuestScope::Individual, 2, 0),
                new_quest("hunt", QuestScope::Shared, 0, 1),
            ]),
        );

        assert_eq!(log.entry("gold").expect("assigned").progress, QuestProgress::Own(9));
        let hunt = log.entry("hunt").expect("assigned");
        assert!(hunt.completed);
        assert_eq!(hunt.progress, QuestProgress::Shared(10));
    }

    #[test]
    fn everyone_quests_start_with_their_own_counter() {
        let mut log = QuestLog::default();
        let mut banner = HudBanner::default();

        handle_quest_message(
            &mut log,
            &mut banner,
            assigned(vec![new_quest("gold", QuestScope::Everyone, 3, 0)]),
        );

        assert_eq!(
            log.entry("gold").expect("assigned").progress,
            QuestProgress::Everyone {
                own: 3,
                players_done: 0,
                players_total: 0
            }
        );
    }

    #[test]
    fn other_messages_are_handed_back() {
        let mut log = QuestLog::default();
        let mut banner = HudBanner::default();

        let returned = handle_quest_message(&mut log, &mut banner, ServerMessage::Firework(SFirework { seed: 7 }));

        assert!(matches!(returned, Some(ServerMessage::Firework(SFirework { seed: 7 }))));
        assert!(banner.pending_texts().is_empty());
    }
}
