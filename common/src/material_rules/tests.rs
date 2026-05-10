use super::MaterialRules;

#[test]
fn default_material_rules_load() {
    MaterialRules::load_default().expect("default per-segment materials should load");
}
