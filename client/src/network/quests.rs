use bevy::prelude::*;
use common::protocol::*;

use super::context::ServerMessageContext;
use crate::{
    audio::play_sound,
    ui::{BannerMessage, HudBanner, QuestLog},
};

pub(super) fn handle_quest_updates_message(
    message: SQuestUpdates,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    if apply_quest_updates(&mut context.quest_log, &mut context.banner, message) {
        play_sound(
            commands,
            &context.assets.asset_server,
            context.assets.asset_set.player_sound("quest_completed"),
        );
    }
}

fn apply_quest_updates(quest_log: &mut QuestLog, banner: &mut HudBanner, message: SQuestUpdates) -> bool {
    let mut announcements = Vec::new();
    let mut completed_any = false;
    for update in message.updates {
        let reason = update.reason;
        let change = match quest_log.apply_state(update.quest) {
            Ok(change) => change,
            Err(error) => {
                error!("invalid quest update from server: {error}");
                continue;
            }
        };
        match reason {
            // A late joiner also receives finished group quests; those are
            // not something to go and do.
            QuestUpdateReason::Assigned | QuestUpdateReason::Progressed
                if change.inserted && !change.became_completed =>
            {
                announcements.push(change.announcement);
            }
            QuestUpdateReason::Completed if change.became_completed => {
                banner.push(BannerMessage::QuestCompleted(change.completed_text));
                completed_any = true;
            }
            _ => {}
        }
    }
    if !announcements.is_empty() {
        banner.push(BannerMessage::QuestAnnouncement(announcements.join("\n")));
    }
    completed_any
}

#[cfg(test)]
#[path = "tests/quests.rs"]
mod tests;
