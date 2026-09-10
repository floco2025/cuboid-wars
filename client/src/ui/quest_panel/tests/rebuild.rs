use super::{super::test_support::*, *};
use common::protocol::QuestScope;

#[test]
fn counter_and_scope_note_by_progress() {
    let own = entry("Gold Rush", QuestScope::Individual, 7, 10, 0);
    assert_eq!(quest_counter(&own), "7/10");
    assert_eq!(scope_note(&own), None);

    let shared = entry("Hunt", QuestScope::Shared, 2, 4, 0);
    assert_eq!(quest_counter(&shared), "2/4");
    assert_eq!(scope_note(&shared).as_deref(), Some("shared progress"));

    let mut everyone = entry("Gold Rush", QuestScope::Everyone, 7, 10, 0);
    everyone.progress = QuestProgress::Everyone {
        own: 7,
        players_done: 2,
        players_total: 3,
    };
    assert_eq!(quest_counter(&everyone), "7/10");
    assert_eq!(scope_note(&everyone).as_deref(), Some("2 of 3 players done"));
}

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
    done.record_completion(common::protocol::QuestId("a".to_owned()));
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
    counted.apply_group_status(&[common::protocol::QuestGroupStatus {
        id: common::protocol::QuestId("a".to_owned()),
        completed: false,
        progress: common::protocol::QuestGroupProgress::Everyone {
            players_done: 1,
            players_total: 3,
        },
    }]);
    assert_ne!(quest_panel_content_hash(&counted), everyone_hash);
}
