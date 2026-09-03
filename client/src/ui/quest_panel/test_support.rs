use common::protocol::{QuestId, QuestScope};

use super::quest_log::{QuestEntry, QuestLog, QuestProgress};

pub fn entry(title: &str, scope: QuestScope, progress: u32, threshold: u32, order: u32) -> QuestEntry {
    QuestEntry {
        title: title.to_owned(),
        description: format!("{title} description"),
        completed_text: format!("{title} complete"),
        threshold,
        progress: QuestProgress::new(scope, progress),
        completed: false,
        order,
    }
}

pub fn log(items: Vec<(&str, QuestEntry)>) -> QuestLog {
    let mut log = QuestLog::default();
    for (id, entry) in items {
        assert!(log.assign(QuestId(id.to_owned()), entry));
    }
    log
}
