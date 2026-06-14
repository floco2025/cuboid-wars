use std::collections::HashMap;

use bevy::prelude::Resource;
use common::protocol::QuestId;

// A quest this client is tracking. The panel renders the short `title` plus
// progress; `description` is kept so the announcement banner can be re-shown
// on respawn (title + description), not just at the initial assignment.
#[derive(Debug, Clone)]
pub struct QuestEntry {
    pub title: String,
    pub description: String,
    pub progress: u32,
    pub threshold: u32,
    // Kept (not removed) once done so the panel can show it completed.
    pub completed: bool,
    // Catalog rank from the server (`gameplay.json` order); the display order
    // for the panel and respawn announcement.
    pub order: u32,
}

// Every quest this client has been assigned this session, keyed by id.
// Populated by the `SQuestsAssigned` handler, advanced by `SQuestProgress`,
// and marked done by `SQuestCompleted`. The quest panel rebuilds from it.
#[derive(Resource, Default)]
pub struct QuestLog {
    pub entries: HashMap<QuestId, QuestEntry>,
}

impl QuestLog {
    // Entries in display order: by catalog `order`, then id as a stable
    // tiebreak. The single ordering used by the panel, its content hash, and
    // the respawn announcement so they never disagree.
    #[must_use]
    pub fn sorted(&self) -> Vec<(&QuestId, &QuestEntry)> {
        let mut entries: Vec<_> = self.entries.iter().collect();
        entries.sort_by(|(a_id, a), (b_id, b)| a.order.cmp(&b.order).then_with(|| a_id.0.cmp(&b_id.0)));
        entries
    }
}
