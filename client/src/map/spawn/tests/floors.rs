use super::*;

fn floor() -> Floor {
    Floor {
        x1: -3.0,
        z1: -1.0,
        x2: 3.0,
        z2: 1.0,
        y: 2.0,
        thickness: 0.4,
        level: 1,
        carrier: CarrierId::WORLD,
    }
}

fn terrain(x: f32) -> TerrainCell {
    TerrainCell {
        x,
        y: 2.0,
        z: 0.0,
        level: 1,
        carrier: CarrierId::WORLD,
    }
}

#[test]
fn terrain_cells_remove_the_old_floor_top_instead_of_overlaying_it() {
    let visible = top_rectangles_without_terrain(&floor(), &[terrain(-2.0), terrain(0.0)], 2.0);
    assert_eq!(visible, vec![[1.0, -1.0, 3.0, 1.0]]);
}

#[test]
fn cuts_ignore_other_levels_and_carriers() {
    let mut other_level = terrain(0.0);
    other_level.level = 2;
    let mut other_carrier = terrain(0.0);
    other_carrier.carrier = CarrierId::from_carried_index(0);
    assert_eq!(
        top_rectangles_without_terrain(&floor(), &[other_level, other_carrier], 2.0),
        vec![[-3.0, -1.0, 3.0, 1.0]]
    );
}

#[test]
fn terrain_material_omits_the_complete_standard_top_including_trim() {
    let mut materials = FaceMaterials::uniform("slab");
    materials.top = TERRAIN_MATERIAL.to_owned();
    assert!(standard_top_rectangles(&floor(), &materials, &[terrain(0.0)], 2.0).is_empty());
}

#[test]
fn debug_rendering_keeps_the_terrain_top() {
    let mut materials = FaceMaterials::uniform("slab");
    materials.top = TERRAIN_MATERIAL.to_owned();
    assert_eq!(
        standard_top_rectangles(&floor(), &materials, &[], 2.0),
        vec![[-3.0, -1.0, 3.0, 1.0]]
    );
}
