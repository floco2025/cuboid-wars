use bevy::prelude::*;

use crate::{
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
    client_settings: &ClientSettings,
    pending_banner: &mut PendingBanner,
    msg: SQuestsAssigned,
) {
    let mut lines = Vec::new();
    for quest in msg.quests {
        if quest_log.entries.contains_key(&quest.id) {
            continue;
        }
        lines.push(format!("{}: {}", quest.title, quest.description));
        quest_log.entries.insert(
            quest.id,
            QuestEntry {
                title: quest.title,
                description: quest.description,
                progress: quest.progress,
                threshold: quest.threshold,
                completed: false,
                order: quest.order,
            },
        );
    }
    if lines.is_empty() {
        return;
    }
    pending_banner.set(
        lines.join("\n"),
        client_settings.hud.banner.quest_announcement_duration_secs,
    );
}

// A quest's progress advanced. Carries the absolute value, so keep the max to
// ignore a reordered/stale update. A progress message for an unknown id (e.g.
// arriving before its assignment batch) is ignored — the assignment seeds it.
pub fn handle_quest_progress_message(quest_log: &mut QuestLog, msg: SQuestProgress) {
    if let Some(entry) = quest_log.entries.get_mut(&msg.id) {
        entry.progress = entry.progress.max(msg.progress);
    }
}

// Server says the local client just completed a quest. Mark it done in the
// panel (kept, shown completed), fire the completion banner, and play the win
// sound.
pub fn handle_quest_completed_message(
    commands: &mut Commands,
    quest_log: &mut QuestLog,
    client_settings: &ClientSettings,
    pending_banner: &mut PendingBanner,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    msg: SQuestCompleted,
) {
    if let Some(entry) = quest_log.entries.get_mut(&msg.id) {
        entry.progress = entry.threshold;
        entry.completed = true;
    }
    pending_banner.set(
        msg.completed_text,
        client_settings.hud.banner.quest_completed_duration_secs,
    );
    commands.spawn((
        AudioPlayer::new(asset_server.load(asset_set.player_sound("quest_completed").to_owned())),
        PlaybackSettings::DESPAWN,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quest_log_with(id: &str, progress: u32, threshold: u32) -> QuestLog {
        let mut log = QuestLog::default();
        log.entries.insert(
            QuestId(id.to_owned()),
            QuestEntry {
                title: "Gold".to_owned(),
                description: "collect gold".to_owned(),
                progress,
                threshold,
                completed: false,
                order: 0,
            },
        );
        log
    }

    #[test]
    fn quest_progress_keeps_max_and_ignores_unknown_id() {
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

        // An update for an unknown quest is a no-op (doesn't insert).
        handle_quest_progress_message(
            &mut log,
            SQuestProgress {
                id: QuestId("unknown".to_owned()),
                progress: 9,
            },
        );
        assert_eq!(log.entries.len(), 1);
    }
}
