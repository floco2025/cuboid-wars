use std::collections::HashMap;

use bevy::prelude::Resource;
use common::protocol::{QuestGroupProgress, QuestGroupStatus, QuestId, QuestInitialProgress, QuestScope};

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
    pub const fn from_initial(progress: QuestInitialProgress) -> Self {
        match progress {
            QuestInitialProgress::Individual { progress } => Self::Own(progress),
            QuestInitialProgress::Shared { progress } => Self::Shared(progress),
            QuestInitialProgress::Everyone {
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

    fn raise_progress(&mut self, value: u32) {
        let counter = self.progress.counter_mut();
        *counter = (*counter).max(value);
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
}

// State events can beat the assignment that carries a quest's display data;
// until it lands, only the counter and completion are remembered.
enum QuestState {
    Pending {
        progress: u32,
        group_progress: Option<QuestGroupProgress>,
        completed: bool,
    },
    Assigned(QuestEntry),
}

impl Default for QuestState {
    fn default() -> Self {
        Self::Pending {
            progress: 0,
            group_progress: None,
            completed: false,
        }
    }
}

#[derive(Resource, Default)]
pub struct QuestLog {
    quests: HashMap<QuestId, QuestState>,
}

impl QuestLog {
    // False for an id already assigned.
    pub fn assign(&mut self, id: QuestId, mut entry: QuestEntry) -> bool {
        match self.quests.get(&id) {
            Some(QuestState::Assigned(_)) => return false,
            Some(QuestState::Pending {
                progress,
                group_progress,
                completed,
            }) => {
                entry.raise_progress(*progress);
                if let Some(group_progress) = group_progress {
                    entry.apply_group_progress(group_progress);
                }
                if *completed {
                    entry.complete();
                }
            }
            None => {}
        }
        self.quests.insert(id, QuestState::Assigned(entry));
        true
    }

    // Absolute value; the max is kept so a stale update can't regress it.
    pub fn record_progress(&mut self, id: QuestId, value: u32) {
        match self.quests.entry(id).or_default() {
            QuestState::Assigned(entry) => entry.raise_progress(value),
            QuestState::Pending { progress, .. } => *progress = (*progress).max(value),
        }
    }

    pub fn record_completion(&mut self, id: QuestId) {
        match self.quests.entry(id).or_default() {
            QuestState::Assigned(entry) => entry.complete(),
            QuestState::Pending { completed, .. } => *completed = true,
        }
    }

    // Group state from the snapshot. Player counts are set, not merged —
    // they drop when a finished player leaves.
    pub fn apply_group_status(&mut self, statuses: &[QuestGroupStatus]) {
        for status in statuses {
            match self.quests.entry(status.id.clone()).or_default() {
                QuestState::Assigned(entry) => entry.apply_group_progress(&status.progress),
                QuestState::Pending {
                    progress,
                    group_progress,
                    ..
                } => {
                    if let QuestGroupProgress::Shared { progress: shared } = &status.progress {
                        *progress = (*progress).max(*shared);
                    }
                    *group_progress = Some(status.progress.clone());
                }
            }
            if status.completed {
                self.record_completion(status.id.clone());
            }
        }
    }

    // Assigned quests in display order: catalog `order`, then id as a stable
    // tiebreak.
    #[must_use]
    pub fn sorted(&self) -> Vec<(&QuestId, &QuestEntry)> {
        let mut entries: Vec<_> = self
            .quests
            .iter()
            .filter_map(|(id, state)| match state {
                QuestState::Assigned(entry) => Some((id, entry)),
                QuestState::Pending { .. } => None,
            })
            .collect();
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
        match self.quests.get(&QuestId(id.to_owned()))? {
            QuestState::Assigned(entry) => Some(entry),
            QuestState::Pending { .. } => None,
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

    #[test]
    fn assign_rejects_a_known_id() {
        let mut log = log(vec![("gold", entry("Gold", QuestScope::Individual, 0, 10, 0))]);
        assert!(!log.assign(id("gold"), entry("Gold", QuestScope::Individual, 0, 10, 0)));
    }

    #[test]
    fn progress_keeps_the_max() {
        let mut log = log(vec![("gold", entry("Gold", QuestScope::Individual, 3, 10, 0))]);

        log.record_progress(id("gold"), 4);
        log.record_progress(id("gold"), 2);

        assert_eq!(log.entry("gold").expect("assigned").progress, QuestProgress::Own(4));
    }

    #[test]
    fn progress_before_assignment_merges_into_the_entry() {
        let mut log = QuestLog::default();
        log.record_progress(id("gold"), 9);
        assert!(log.sorted().is_empty());

        assert!(log.assign(id("gold"), entry("Gold", QuestScope::Individual, 2, 10, 0)));

        assert_eq!(log.entry("gold").expect("assigned").progress, QuestProgress::Own(9));
    }

    #[test]
    fn completion_before_assignment_marks_the_entry_complete() {
        let mut log = QuestLog::default();
        log.record_completion(id("gold"));

        assert!(log.assign(id("gold"), entry("Gold", QuestScope::Individual, 3, 10, 0)));

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
    fn group_status_completion_before_assignment_is_buffered() {
        let mut log = QuestLog::default();
        log.apply_group_status(&[status("show", true, QuestGroupProgress::Shared { progress: 1 })]);
        assert!(log.sorted().is_empty());

        assert!(log.assign(id("show"), entry("Show", QuestScope::Shared, 0, 1, 0)));

        let entry = log.entry("show").expect("assigned");
        assert!(entry.completed);
        assert_eq!(entry.progress, QuestProgress::Shared(1));
    }

    #[test]
    fn everyone_counts_before_assignment_are_buffered() {
        let mut log = QuestLog::default();
        log.apply_group_status(&[status("gold", false, everyone(2, 3))]);

        assert!(log.assign(id("gold"), entry("Gold", QuestScope::Everyone, 1, 10, 0)));

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
