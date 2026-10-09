use rand::{SeedableRng, rngs::StdRng};

use super::choose_item_type;
use crate::{config::RandomItemsConfig, items::RandomItems, map::MapConfig, test_geometry::geometry};
use common::protocol::ItemType;

#[test]
fn random_item_selection_follows_relative_weights() {
    let config = RandomItemsConfig {
        weights: [
            ("single_shot".to_owned(), 0.5),
            ("missile_pack".to_owned(), 1.5),
            ("gold".to_owned(), 3.0),
            ("speed".to_owned(), 0.0),
        ]
        .into(),
        max_number: 3,
        despawn_secs: 10.0,
    };
    let random = RandomItems::from_config(Some(&config));
    let mut rng = StdRng::seed_from_u64(42);
    let mut counts = [0; 3];
    for _ in 0..20_000 {
        match choose_item_type(&mut rng, &random.pool).expect("random item selection returned no item") {
            ItemType::SingleShotPowerUp => counts[0] += 1,
            ItemType::MissilePack => counts[1] += 1,
            ItemType::Gold => counts[2] += 1,
            picked => panic!("disabled or omitted item selected: {picked:?}"),
        }
    }
    for (count, expected) in counts.into_iter().zip([0.1, 0.3, 0.6]) {
        let frequency = f64::from(count) / 20_000.0;
        assert!(
            (frequency - expected).abs() < 0.02,
            "{frequency} differs from {expected}"
        );
    }
}

#[test]
fn zero_weight_items_are_excluded_from_selection_and_map_availability() {
    let config = RandomItemsConfig {
        weights: [("single_shot".to_owned(), 0.0), ("gold".to_owned(), 1.0)].into(),
        max_number: 3,
        despawn_secs: 10.0,
    };
    let random = RandomItems::from_config(Some(&config));
    let map = MapConfig::for_grid(Vec::new(), geometry(1, 1));
    let available = map.available_items(random.pool.iter().map(|&(item_type, _)| item_type));
    assert_eq!(available.0, vec![ItemType::Gold]);
    let mut rng = StdRng::seed_from_u64(42);
    for _ in 0..50 {
        assert_eq!(choose_item_type(&mut rng, &random.pool), Some(ItemType::Gold));
    }
}
