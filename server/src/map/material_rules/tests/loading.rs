use super::super::MaterialRules;
use crate::{map::generation::map_path, test_geometry::sizes};

#[test]
fn hotel_material_rules_build_from_def() {
    let map_def = crate::map::definition::load_map(&map_path("hotel")).expect("hotel map should load");
    let rules = MaterialRules::from_def(&map_def.geometry, sizes());
    assert!(!rules.segments.floors.is_empty());
    assert!(!rules.segments.walls.is_empty());
    assert!(!rules.segments.ramps.is_empty());
}
