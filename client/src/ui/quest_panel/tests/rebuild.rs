use common::protocol::{QuestGroupProgress, QuestGroupStatus, QuestId, QuestScope};

use super::{super::test_support::*, *};

#[test]
fn content_hash_is_independent_of_insertion_order() {
    let forward = log(vec![
        ("a", entry("Gold", QuestScope::Individual, 3, 10, 0)),
        ("b", entry("Hunt", QuestScope::Individual, 1, 4, 0)),
    ]);
    let reverse = log(vec![
        ("b", entry("Hunt", QuestScope::Individual, 1, 4, 0)),
        ("a", entry("Gold", QuestScope::Individual, 3, 10, 0)),
    ]);

    assert_eq!(quest_panel_content_hash(&forward), quest_panel_content_hash(&reverse));
}

#[test]
fn content_hash_changes_on_rendered_fields() {
    let base = log(vec![("a", entry("Gold", QuestScope::Individual, 3, 10, 0))]);
    let base_hash = quest_panel_content_hash(&base);

    let advanced = log(vec![("a", entry("Gold", QuestScope::Individual, 4, 10, 0))]);
    assert_ne!(quest_panel_content_hash(&advanced), base_hash);

    let mut done = log(vec![("a", entry("Gold", QuestScope::Individual, 3, 10, 0))]);
    done.record_completion(QuestId("a".to_owned()));
    assert_ne!(quest_panel_content_hash(&done), base_hash);

    let retitled = log(vec![("a", entry("Gold Rush", QuestScope::Individual, 3, 10, 0))]);
    assert_ne!(quest_panel_content_hash(&retitled), base_hash);

    let rethreshold = log(vec![("a", entry("Gold", QuestScope::Individual, 3, 12, 0))]);
    assert_ne!(quest_panel_content_hash(&rethreshold), base_hash);

    let joined = log(vec![
        ("a", entry("Gold", QuestScope::Individual, 3, 10, 0)),
        ("b", entry("Hunt", QuestScope::Individual, 0, 4, 0)),
    ]);
    assert_ne!(quest_panel_content_hash(&joined), base_hash);

    let everyone = log(vec![("a", entry("Gold", QuestScope::Everyone, 3, 10, 0))]);
    let everyone_hash = quest_panel_content_hash(&everyone);
    assert_ne!(everyone_hash, base_hash);
    let mut counted = log(vec![("a", entry("Gold", QuestScope::Everyone, 3, 10, 0))]);
    counted.apply_group_status(&[QuestGroupStatus {
        id: QuestId("a".to_owned()),
        completed: false,
        progress: QuestGroupProgress::Everyone {
            players_done: 1,
            players_total: 3,
        },
    }]);
    assert_ne!(quest_panel_content_hash(&counted), everyone_hash);
}
