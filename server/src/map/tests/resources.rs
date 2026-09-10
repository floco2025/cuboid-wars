use super::*;
use common::protocol::BarrierKindId;

fn key(kind: u16) -> PlacedItem {
    PlacedItem {
        carrier: CarrierId::WORLD,
        level: 0,
        col: 0,
        row: 0,
        item_type: ItemType::Key(BarrierKindId(kind)),
    }
}

#[test]
fn key_kinds_are_sorted_and_deduplicated() {
    let config = MapConfig {
        placed_items: vec![
            key(2),
            PlacedItem {
                item_type: ItemType::Gold,
                ..key(0)
            },
            key(0),
            key(2),
        ],
        ..MapConfig::for_grid(Vec::new(), crate::test_geometry::geometry(1, 1))
    };

    let items = config.available_items(&[]);
    assert_eq!(items.key_kinds(), [BarrierKindId(0), BarrierKindId(2)]);
}

#[test]
fn available_items_include_placed_and_random_pickups_without_duplicates() {
    let config = MapConfig {
        placed_items: vec![PlacedItem {
            item_type: ItemType::PortalGunPowerUp,
            ..key(0)
        }],
        ..MapConfig::for_grid(Vec::new(), crate::test_geometry::geometry(1, 1))
    };
    let items = config.available_items(&[
        ItemType::MissilePack,
        ItemType::PortalGunPowerUp,
        ItemType::SingleShotPowerUp,
        ItemType::MultiShotPowerUp,
    ]);
    assert_eq!(
        items.0,
        [
            ItemType::SingleShotPowerUp,
            ItemType::MultiShotPowerUp,
            ItemType::MissilePack,
            ItemType::PortalGunPowerUp,
        ]
    );
}
