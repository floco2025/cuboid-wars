use super::*;
use crate::{map::PlacedItem, test_geometry::geometry};
use common::protocol::{CarrierId, QuestId, QuestScope};

#[test]
fn gold_quests_require_placed_gold_or_positive_random_weight() {
    let mut map = MapConfig::for_grid(Vec::new(), geometry(1, 1));
    let quests = [Quest {
        id: QuestId("collect_gold".to_owned()),
        kind: QuestKind::Gold,
        scope: QuestScope::Individual,
        requires: None,
        actor_kind: None,
        threshold: 1,
        points: 100,
        title: "Collect gold".to_owned(),
        description: "Find a coin".to_owned(),
        completed_text: "Coin collected".to_owned(),
    }];
    let mut random = RandomItemsConfig {
        weights: [("gold".to_owned(), 0.0), ("speed".to_owned(), 1.0)].into(),
        max_number: 3,
        despawn_secs: 10.0,
    };
    assert!(validate_map_quests(&quests, &map, Some(&random), None).is_err());
    random.weights.insert("gold".to_owned(), 0.5);
    validate_map_quests(&quests, &map, Some(&random), None).expect("random gold quest rejected");
    random.weights.remove("gold");
    assert!(validate_map_quests(&quests, &map, Some(&random), None).is_err());
    map.placed_items.push(PlacedItem {
        carrier: CarrierId::WORLD,
        level: 0,
        col: 0,
        row: 0,
        item_type: ItemType::Gold,
    });
    random.weights.insert("gold".to_owned(), 0.0);
    validate_map_quests(&quests, &map, Some(&random), None).expect("placed gold quest rejected");
}
