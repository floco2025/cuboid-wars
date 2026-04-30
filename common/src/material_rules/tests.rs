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
            { "level": 2, "walls": { "all": "level-wall" } },
            { "level": 2, "from": [4, 5], "to": [5, 5], "walls": { "all": "single-edge" } }
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
              "level": 2,
              "cols": [5, 5],
              "rows": [5, 8],
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
fn same_specificity_conflicts_panic() {
    let rules = parse_rules(
        r#"
        {
          "material_rules": [
            { "walls": { "all": "fallback" } },
            { "level": 2, "cols": [5, 5], "walls": { "all": "a" } },
            { "level": 2, "cols": [5, 5], "walls": { "all": "b" } }
          ]
        }
        "#,
    );

    let result = std::panic::catch_unwind(|| rules.materials_for_wall_edge(2, [5, 4], [5, 5]));
    assert!(result.is_err());
}
