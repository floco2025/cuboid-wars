use super::*;

fn random_items(weights: &[(&str, f64)]) -> RandomItemsConfig {
    RandomItemsConfig {
        weights: weights
            .iter()
            .map(|&(item, weight)| (item.to_owned(), weight))
            .collect(),
        max_number: 30,
        despawn_secs: 60.0,
    }
}

#[test]
fn the_random_pool_holds_only_known_item_types_and_no_keys() {
    for (item, expected) in [("key", "parameterized by field"), ("banana", "unknown item type")] {
        let err = random_items(&[("speed", 1.0), (item, 1.0)])
            .validate("random_items")
            .expect_err("invalid random item accepted");
        assert!(err.to_string().contains(expected), "{err}");
    }
}

#[test]
fn random_weights_are_finite_and_non_negative_with_a_finite_positive_total() {
    random_items(&[("speed", 0.5), ("gold", 3.0), ("missile_pack", 0.0)])
        .validate("random_items")
        .expect("valid random item weights rejected");
    for weight in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let err = random_items(&[("speed", weight), ("gold", 1.0)])
            .validate("random_items")
            .expect_err("invalid random item weight accepted");
        assert!(err.to_string().contains("random_items.weights.speed"), "{err}");
    }
    for (weights, expected) in [
        (&[][..], "weights must not be empty"),
        (&[("speed", 0.0), ("gold", 0.0)][..], "at least one positive weight"),
        (
            &[("speed", f64::MAX), ("gold", f64::MAX)][..],
            "weights total must be finite",
        ),
    ] {
        let err = random_items(weights)
            .validate("random_items")
            .expect_err("invalid random item pool accepted");
        assert!(err.to_string().contains(expected), "{err}");
    }
}
