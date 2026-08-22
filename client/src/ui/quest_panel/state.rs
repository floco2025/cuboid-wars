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

#[derive(Default)]
struct PendingQuestState {
    max_progress: u32,
    completed: bool,
}

// Every quest this client has been assigned this session, keyed by id. State
// events that arrive before assignment stay pending until display metadata is
// available; the panel renders only `entries`.
#[derive(Resource, Default)]
pub struct QuestLog {
    pub entries: HashMap<QuestId, QuestEntry>,
    pending: HashMap<QuestId, PendingQuestState>,
}

impl QuestLog {
    pub fn assign(&mut self, id: QuestId, mut entry: QuestEntry) -> bool {
        if self.entries.contains_key(&id) {
            return false;
        }
        if let Some(pending) = self.pending.remove(&id) {
            entry.progress = entry.progress.max(pending.max_progress);
            if pending.completed {
                entry.progress = entry.threshold;
                entry.completed = true;
            }
        }
        self.entries.insert(id, entry);
        true
    }

    pub fn record_progress(&mut self, id: QuestId, progress: u32) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.progress = entry.progress.max(progress);
        } else {
            let pending = self.pending.entry(id).or_default();
            pending.max_progress = pending.max_progress.max(progress);
        }
    }

    pub fn record_completion(&mut self, id: QuestId) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.progress = entry.threshold;
            entry.completed = true;
        } else {
            self.pending.entry(id).or_default().completed = true;
        }
    }

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
