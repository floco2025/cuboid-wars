use std::collections::HashMap;

use bevy::prelude::Resource;
use common::protocol::{QuestGroupProgress, QuestGroupStatus, QuestId, QuestScope, QuestState, QuestStateProgress};

fn progress_matches_scope(progress: &QuestStateProgress, scope: QuestScope) -> bool {
    matches!(
        (progress, scope),
        (QuestStateProgress::Individual { .. }, QuestScope::Individual)
            | (QuestStateProgress::Shared { .. }, QuestScope::Shared)
            | (QuestStateProgress::Everyone { .. }, QuestScope::Everyone)
    )
}

// Whose counter a quest shows. `Everyone` carries the own counter next to
// the group tally the snapshot reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuestProgress {
    Own(u32),
    Shared(u32),
    Everyone {
        own: u32,
        players_done: u32,
        players_total: u32,
    },
}

impl QuestProgress {
    #[must_use]
    pub fn new(scope: QuestScope, value: u32) -> Self {
        match scope {
            QuestScope::Individual => Self::Own(value),
            QuestScope::Shared => Self::Shared(value),
            QuestScope::Everyone => Self::Everyone {
                own: value,
                players_done: 0,
                players_total: 0,
            },
        }
    }

    #[must_use]
    pub const fn from_state(progress: QuestStateProgress) -> Self {
        match progress {
            QuestStateProgress::Individual { progress } => Self::Own(progress),
            QuestStateProgress::Shared { progress } => Self::Shared(progress),
            QuestStateProgress::Everyone {
                progress,
                players_done,
                players_total,
            } => Self::Everyone {
                own: progress,
                players_done,
                players_total,
            },
        }
    }

    // The counter shown against the threshold.
    #[must_use]
    pub fn value(self) -> u32 {
        match self {
            Self::Own(value) | Self::Shared(value) => value,
            Self::Everyone { own, .. } => own,
        }
    }

    fn counter_mut(&mut self) -> &mut u32 {
        match self {
            Self::Own(value) | Self::Shared(value) => value,
            Self::Everyone { own, .. } => own,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QuestEntry {
    pub title: String,
    pub description: String,
    pub completed_text: String,
    pub threshold: u32,
    pub progress: QuestProgress,
    // Kept (not removed) once done so the panel can show it completed.
    pub completed: bool,
    // Catalog rank from the server; the display order everywhere.
    pub order: u32,
}

impl QuestEntry {
    #[must_use]
    pub fn announcement(&self) -> String {
        format!("{}: {}", self.title, self.description)
    }

    fn complete(&mut self) {
        *self.progress.counter_mut() = self.threshold;
        self.completed = true;
    }

    fn apply_group_progress(&mut self, progress: &QuestGroupProgress) {
        match (progress, &mut self.progress) {
            (QuestGroupProgress::Shared { progress }, QuestProgress::Shared(current)) => {
                *current = (*current).max(*progress);
            }
            (
                QuestGroupProgress::Everyone {
                    players_done,
                    players_total,
                },
                QuestProgress::Everyone {
                    players_done: current_done,
                    players_total: current_total,
                    ..
                },
            ) => {
                *current_done = *players_done;
                *current_total = *players_total;
            }
            _ => {}
        }
    }

    fn merge(&mut self, incoming: Self) -> anyhow::Result<()> {
        if self.title != incoming.title
            || self.description != incoming.description
            || self.completed_text != incoming.completed_text
            || self.threshold != incoming.threshold
            || self.order != incoming.order
        {
            anyhow::bail!("quest definition changed across updates");
        }
        match (&mut self.progress, incoming.progress) {
            (QuestProgress::Own(current), QuestProgress::Own(next))
            | (QuestProgress::Shared(current), QuestProgress::Shared(next)) => {
                *current = (*current).max(next);
            }
            (
                QuestProgress::Everyone {
                    own,
                    players_done,
                    players_total,
                },
                QuestProgress::Everyone {
                    own: next_own,
                    players_done: next_done,
                    players_total: next_total,
                },
            ) => {
                *own = (*own).max(next_own);
                *players_done = next_done;
                *players_total = next_total;
            }
            _ => anyhow::bail!("quest scope changed across updates"),
        }
        if incoming.completed {
            self.complete();
        }
        Ok(())
    }
}

pub struct QuestStateChange {
    pub inserted: bool,
    pub became_completed: bool,
    pub announcement: String,
    pub completed_text: String,
}

#[derive(Resource, Default)]
pub struct QuestLog {
    quests: HashMap<QuestId, QuestEntry>,
}

impl QuestLog {
    pub fn apply_state(&mut self, state: QuestState) -> anyhow::Result<QuestStateChange> {
        if state.id.0.is_empty() {
            anyhow::bail!("quest update contains an empty id");
        }
        if state.title.is_empty() || state.description.is_empty() || state.completed_text.is_empty() {
            anyhow::bail!("quest {:?} has empty display text", state.id.0);
        }
        if state.threshold == 0 {
            anyhow::bail!("quest {:?} has a zero threshold", state.id.0);
        }
        if !progress_matches_scope(&state.status.progress, state.scope) {
            anyhow::bail!("quest {:?} progress does not match its scope", state.id.0);
        }
        let id = state.id;
        let mut incoming = QuestEntry {
            title: state.title,
            description: state.description,
            completed_text: state.completed_text,
            threshold: state.threshold,
            progress: QuestProgress::from_state(state.status.progress),
            completed: state.status.completed,
            order: state.order,
        };
        if incoming.completed {
            incoming.complete();
        }
        let announcement = incoming.announcement();
        let completed_text = incoming.completed_text.clone();
        let inserted = !self.quests.contains_key(&id);
        let was_completed = self.quests.get(&id).is_some_and(|entry| entry.completed);
        if let Some(entry) = self.quests.get_mut(&id) {
            entry.merge(incoming)?;
        } else {
            self.quests.insert(id.clone(), incoming);
        }
        let became_completed = !was_completed && self.quests.get(&id).is_some_and(|entry| entry.completed);
        Ok(QuestStateChange {
            inserted,
            became_completed,
            announcement,
            completed_text,
        })
    }

    // Group state from the snapshot. Player counts are set, not merged —
    // they drop when a finished player leaves.
    pub fn apply_group_status(&mut self, statuses: &[QuestGroupStatus]) {
        for status in statuses {
            let Some(entry) = self.quests.get_mut(&status.id) else {
                continue;
            };
            entry.apply_group_progress(&status.progress);
            if status.completed {
                entry.complete();
            }
        }
    }

    // Assigned quests in authored `order`, then id as a stable
    // tiebreak.
    #[must_use]
    pub fn sorted(&self) -> Vec<(&QuestId, &QuestEntry)> {
        let mut entries: Vec<_> = self.quests.iter().collect();
        entries.sort_by(|(a_id, a), (b_id, b)| a.order.cmp(&b.order).then_with(|| a_id.0.cmp(&b_id.0)));
        entries
    }

    // What a respawning player is reminded of: the announcements of every
    // quest still to do, in display order.
    #[must_use]
    pub fn reminder(&self) -> Option<String> {
        let lines: Vec<String> = self
            .sorted()
            .into_iter()
            .filter(|(_, entry)| !entry.completed && entry.progress.value() < entry.threshold)
            .map(|(_, entry)| entry.announcement())
            .collect();
        (!lines.is_empty()).then(|| lines.join("\n"))
    }

    #[cfg(test)]
    pub fn entry(&self, id: &str) -> Option<&QuestEntry> {
        self.quests.get(&QuestId(id.to_owned()))
    }

    #[cfg(test)]
    pub fn assign(&mut self, id: QuestId, entry: QuestEntry) -> bool {
        if self.quests.contains_key(&id) {
            return false;
        }
        self.quests.insert(id, entry);
        true
    }

    #[cfg(test)]
    pub fn record_completion(&mut self, id: QuestId) {
        if let Some(entry) = self.quests.get_mut(&id) {
            entry.complete();
        }
    }
}

#[cfg(test)]
#[path = "tests/quest_log.rs"]
mod tests;
