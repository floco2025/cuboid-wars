use super::MaterialRules;

fn parse_rules(json: &str) -> MaterialRules {
    serde_json::from_str(json).expect("material rules should parse")
}

#[test]
fn specific_wall_rule_wins_over_level_default() {
    let rules = parse_rules(
        r#"
        {
          "material_rules": [
            { "walls": { "all": "fallback" } },
            { "levels": 2, "walls": { "all": "level-wall" } },
            { "levels": 2, "from": [4, 5], "to": [5, 5], "walls": { "all": "single-edge" } }
          ]
        }
        "#,
    );

    assert_eq!(
        rules.materials_for_wall_edge(2, [4, 5], [5, 5]).primary(),
        "single-edge"
    );
    assert_eq!(rules.materials_for_wall_edge(2, [5, 5], [6, 5]).primary(), "level-wall");
    assert_eq!(rules.materials_for_wall_edge(1, [4, 5], [5, 5]).primary(), "fallback");
}

#[test]
fn touching_wall_rule_matches_edges_that_touch_the_scope() {
    let rules = parse_rules(
        r#"
        {
          "material_rules": [
            { "walls": { "all": "fallback" } },
            {
              "levels": 2,
              "edge_cols": [5, 5],
              "edge_rows": [5, 8],
              "touching_walls": {
                "all": "touch-default",
                "east": "touch-east"
              }
            }
          ]
        }
        "#,
    );

    let materials = rules.materials_for_wall_edge(2, [4, 6], [5, 6]);
    assert_eq!(materials.east, "touch-east");
    assert_eq!(materials.west, "touch-default");

    assert_eq!(
        rules.materials_for_wall_edge(2, [5, 6], [6, 6]).primary(),
        "touch-default"
    );
    assert_eq!(rules.materials_for_wall_edge(2, [6, 6], [7, 6]).primary(), "fallback");
}

#[test]
fn rules_can_reference_layers_by_name() {
    let rules = parse_rules(
        r#"
        {
          "level_names": {
            "basement": 0,
            "lobby": 1,
            "rooms-low": 2,
            "rooms-high": 3
          },
          "material_rules": [
            { "walls": { "all": "fallback" } },
            { "levels": "lobby", "walls": { "all": "lobby-wall" } },
            { "levels": ["rooms-low", "rooms-high"], "walls": { "all": "rooms-wall" } }
          ]
        }
        "#,
    );

    assert_eq!(rules.materials_for_wall_edge(1, [4, 5], [5, 5]).primary(), "lobby-wall");
    assert_eq!(rules.materials_for_wall_edge(2, [4, 5], [5, 5]).primary(), "rooms-wall");
    assert_eq!(rules.materials_for_wall_edge(0, [4, 5], [5, 5]).primary(), "fallback");
}

#[test]
fn unknown_layer_name_fails_to_parse() {
    let err = serde_json::from_str::<MaterialRules>(
        r#"
        {
          "level_names": { "basement": 0 },
          "material_rules": [
            { "levels": "lobby", "walls": { "all": "missing" } }
          ]
        }
        "#,
    )
    .expect_err("unknown layer names should be rejected");

    assert!(err.to_string().contains("unknown material layer"));
}

#[test]
fn default_material_rules_parse() {
    MaterialRules::load_default().expect("default material rules should parse");
}

#[test]
fn same_specificity_conflicts_panic() {
    let rules = parse_rules(
        r#"
        {
          "material_rules": [
            { "walls": { "all": "fallback" } },
            { "levels": 2, "edge_cols": [5, 5], "walls": { "all": "a" } },
            { "levels": 2, "edge_cols": [5, 5], "walls": { "all": "b" } }
          ]
        }
        "#,
    );

    let result = std::panic::catch_unwind(|| rules.materials_for_wall_edge(2, [5, 4], [5, 5]));
    assert!(result.is_err());
}
