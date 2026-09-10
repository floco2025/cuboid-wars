use super::*;
use crate::{
    config::QuestKind,
    players::PlayerQuestState,
    quests::test_support::{catalog, join, quest},
};
use common::protocol::{PlayerId, SwitchId};

#[test]
fn group_statuses_list_only_unlocked_group_quests() {
    let config = catalog(vec![
        quest("solo", QuestKind::Gold, QuestScope::Individual, 3, None),
        quest("pool", QuestKind::ActorKills, QuestScope::Shared, 4, None),
        quest("gold", QuestKind::Gold, QuestScope::Everyone, 2, None),
        quest("later", QuestKind::Gold, QuestScope::Shared, 1, Some("gold")),
    ]);
    let quest_catalog = QuestCatalog::from_config(&config);
    let board = QuestBoard::from_catalog(&quest_catalog, None);
    let mut players = PlayerMap::default();
    let _alice = join(&mut players, 1, &quest_catalog, &board);
    let _bob = join(&mut players, 2, &quest_catalog, &board);
    players
        .get_mut(&PlayerId(1))
        .expect("alice")
        .session
        .quest_states
        .insert(QuestId("gold".to_owned()), PlayerQuestState::Everyone { progress: 2 });

    let statuses = board.group_statuses(&quest_catalog, &players);

    let ids: Vec<&str> = statuses.iter().map(|status| status.id.0.as_str()).collect();
    assert_eq!(ids, ["pool", "gold"]);
    assert_eq!(statuses[0].progress, QuestGroupProgress::Shared { progress: 0 });
    assert_eq!(
        statuses[1].progress,
        QuestGroupProgress::Everyone {
            players_done: 1,
            players_total: 2
        }
    );
    assert!(!statuses[1].completed);
}

#[test]
fn locked_switches_follow_the_claiming_quests() {
    let config = catalog(vec![
        quest("gold", QuestKind::Gold, QuestScope::Everyone, 1, None),
        quest("show", QuestKind::Fireworks, QuestScope::Shared, 1, Some("gold")),
    ]);
    let quest_catalog = QuestCatalog::from_config(&config);
    let mut board = QuestBoard::from_catalog(&quest_catalog, Some(SwitchId(2)));
    assert_eq!(board.locked_switches(), [SwitchId(2)]);
    board.unlock(&QuestId("show".to_owned()));
    assert!(board.locked_switches().is_empty());

    let unclaimed_config = catalog(vec![quest("gold", QuestKind::Gold, QuestScope::Everyone, 1, None)]);
    let unclaimed_catalog = QuestCatalog::from_config(&unclaimed_config);
    let unclaimed = QuestBoard::from_catalog(&unclaimed_catalog, Some(SwitchId(2)));
    assert!(unclaimed.locked_switches().is_empty());
}
