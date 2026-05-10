use super::{FaceMaterials, rules::MaterialRule};

pub(super) fn resolve_face_materials(
    rules: &[MaterialRule],
    grid_cols: i32,
    grid_rows: i32,
    matches: impl Fn(&MaterialRule) -> bool,
) -> FaceMaterials {
    let mut best: Option<(&MaterialRule, u16)> = None;
    for rule in rules.iter().filter(|rule| matches(rule)) {
        let specificity = rule.specificity(grid_cols, grid_rows);
        match best {
            None => best = Some((rule, specificity)),
            Some((_, best_specificity)) if specificity > best_specificity => {
                best = Some((rule, specificity));
            }
            Some((best_rule, best_specificity)) if specificity == best_specificity => {
                let best_materials = best_rule.face_materials();
                let rule_materials = rule.face_materials();
                if best_materials == rule_materials {
                    continue;
                }
                panic!(
                    "conflicting asset material rules with same specificity: {:?} and {:?}",
                    best_materials, rule_materials
                );
            }
            Some(_) => {}
        }
    }

    best.map(|(rule, _)| rule.face_materials())
        .expect("asset material rule list must have a fallback rule")
}

pub(super) fn resolve_item_material(
    rules: &[MaterialRule],
    grid_cols: i32,
    grid_rows: i32,
    matches: impl Fn(&MaterialRule) -> bool,
) -> &str {
    let mut best: Option<(&MaterialRule, u16)> = None;
    for rule in rules.iter().filter(|rule| matches(rule)) {
        let specificity = rule.specificity(grid_cols, grid_rows);
        match best {
            None => best = Some((rule, specificity)),
            Some((_, best_specificity)) if specificity > best_specificity => {
                best = Some((rule, specificity));
            }
            Some((best_rule, best_specificity))
                if specificity == best_specificity && rule.item_material() != best_rule.item_material() =>
            {
                panic!(
                    "conflicting asset item material rules with same specificity: {:?} and {:?}",
                    best_rule.item_material(),
                    rule.item_material()
                );
            }
            Some(_) => {}
        }
    }

    best.map(|(rule, _)| rule.item_material())
        .expect("asset item material rule list must have a fallback rule")
}
