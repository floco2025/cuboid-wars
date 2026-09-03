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
mod tests {
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
}
