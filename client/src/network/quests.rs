use crate::constants::{BANNER_QUEST_ANNOUNCEMENT_SECS, BANNER_QUEST_COMPLETED_SECS};
use bevy::prelude::*;

use crate::{
    audio::play_sound,
    config::{AssetSet, ClientSettings},
    ui::{PendingBanner, QuestEntry, QuestLog},
};
use common::protocol::*;

// Server assigned the local client a batch of quests (at login right after
// `SInit`, or in-game from a quest-giver). Seed each new quest into the panel's
// log and show ONE combined announcement banner — title + description per
// quest. Already-known ids are skipped (defensive; the server only sends
// genuinely-new quests).
pub fn handle_quests_assigned_message(
    quest_log: &mut QuestLog,
    _client_settings: &ClientSettings,
    pending_banner: &mut PendingBanner,
    msg: SQuestsAssigned,
) {
    let mut lines = Vec::new();
    for quest in msg.quests {
        let announcement = format!("{}: {}", quest.title, quest.description);
        if quest_log.assign(
            quest.id,
            QuestEntry {
                title: quest.title,
                description: quest.description,
                progress: quest.progress,
                threshold: quest.threshold,
                completed: false,
                order: quest.order,
            },
        ) {
            lines.push(announcement);
        }
    }
    if lines.is_empty() {
        return;
    }
    pending_banner.set(lines.join("\n"), BANNER_QUEST_ANNOUNCEMENT_SECS);
}

// A quest's progress advanced. Carries the absolute value, so keep the max to
// ignore a reordered/stale update. Updates that beat their assignment are
// retained until the assignment provides the display metadata.
pub fn handle_quest_progress_message(quest_log: &mut QuestLog, msg: SQuestProgress) {
    quest_log.record_progress(msg.id, msg.progress);
}

// Server says the local client just completed a quest. Mark it done in the
// panel (kept, shown completed), fire the completion banner, and play the win
// sound.
pub fn handle_quest_completed_message(
    commands: &mut Commands,
    quest_log: &mut QuestLog,
    _client_settings: &ClientSettings,
    pending_banner: &mut PendingBanner,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    msg: SQuestCompleted,
) {
    quest_log.record_completion(msg.id);
    pending_banner.set(msg.completed_text, BANNER_QUEST_COMPLETED_SECS);
    play_sound(commands, asset_server, asset_set.player_sound("quest_completed"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quest_entry(progress: u32, threshold: u32) -> QuestEntry {
        QuestEntry {
            title: "Gold".to_owned(),
            description: "collect gold".to_owned(),
            progress,
            threshold,
            completed: false,
            order: 0,
        }
    }

    fn quest_log_with(id: &str, progress: u32, threshold: u32) -> QuestLog {
        let mut log = QuestLog::default();
        assert!(log.assign(QuestId(id.to_owned()), quest_entry(progress, threshold)));
        log
    }

    #[test]
    fn quest_progress_keeps_max() {
        let mut log = quest_log_with("collect_gold", 3, 10);

        // Advancing update applies.
        handle_quest_progress_message(
            &mut log,
            SQuestProgress {
                id: QuestId("collect_gold".to_owned()),
                progress: 4,
            },
        );
        assert_eq!(log.entries[&QuestId("collect_gold".to_owned())].progress, 4);

        // A stale, lower value is discarded (absolute value + max guard).
        handle_quest_progress_message(
            &mut log,
            SQuestProgress {
                id: QuestId("collect_gold".to_owned()),
                progress: 2,
            },
        );
        assert_eq!(log.entries[&QuestId("collect_gold".to_owned())].progress, 4);
    }

    #[test]
    fn quest_progress_before_assignment_merges_into_assigned_entry() {
        let mut log = QuestLog::default();

        let unknown = QuestId("unknown".to_owned());
        handle_quest_progress_message(
            &mut log,
            SQuestProgress {
                id: unknown.clone(),
                progress: 9,
            },
        );
        assert!(log.entries.is_empty());
        assert!(log.assign(unknown.clone(), quest_entry(2, 10)));
        assert_eq!(log.entries[&unknown].progress, 9);
    }

    #[test]
    fn quest_completion_before_assignment_marks_assigned_entry_complete() {
        let mut log = QuestLog::default();
        let id = QuestId("collect_gold".to_owned());

        log.record_completion(id.clone());
        assert!(log.assign(id.clone(), quest_entry(3, 10)));

        let entry = &log.entries[&id];
        assert_eq!(entry.progress, 10);
        assert!(entry.completed);
    }
}
