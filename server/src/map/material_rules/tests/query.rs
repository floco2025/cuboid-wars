use super::*;
use crate::test_geometry::{CELL, LEVEL_HEIGHT, WALL_THICKNESS, geometry};
use common::protocol::CarrierId;

const HALF: f32 = WALL_THICKNESS / 2.0;

fn faces(inside: &str, face: fn(&mut FaceMaterials) -> &mut String) -> FaceMaterials {
    let mut materials = FaceMaterials::uniform("cap");
    *face(&mut materials) = inside.to_owned();
    materials
}

// A room's north-west corner, two storeys tall: a west wall facing east into
// the room and a north wall facing south into it. The upper sections are
// another material, which a trim capping the lower ones must not take.
fn corner() -> MaterialRules {
    let mut segments = SegmentMaterials::default();
    segments
        .walls
        .insert((0, [0, 0], [0, 1]), faces("west-wall", |m| &mut m.east));
    segments
        .walls
        .insert((0, [0, 0], [1, 0]), faces("north-wall", |m| &mut m.south));
    for edge in [[1, 0], [0, 1]] {
        segments
            .walls
            .insert((1, [0, 0], edge), FaceMaterials::uniform("upper"));
    }
    MaterialRules {
        geometry: geometry(2, 2),
        segments,
    }
}

// The trim capping the west wall's lower section in the corner cell.
fn west_trim(rules: &MaterialRules) -> Floor {
    let x = rules.geometry.cell_to_world_x(0);
    let z = rules.geometry.cell_to_world_z(0);
    Floor {
        x1: x - HALF,
        z1: z - HALF,
        x2: x + HALF,
        z2: z + CELL,
        y: LEVEL_HEIGHT,
        thickness: rules.geometry.floor_thickness(),
        level: 1,
        carrier: CarrierId::WORLD,
    }
}

#[test]
fn a_corner_trim_takes_the_wall_it_caps_not_the_wall_it_meets() {
    let rules = corner();
    let materials = rules.materials_for_floor(&west_trim(&rules));
    assert_eq!(materials.east, "west-wall");
    assert_eq!(materials.south, "cap");
}

#[test]
fn a_trim_beside_a_floor_keeps_its_walls_faces() {
    let mut rules = corner();
    rules.segments.floors.insert((1, 1, 0), FaceMaterials::uniform("floor"));
    assert_eq!(rules.materials_for_floor(&west_trim(&rules)).east, "west-wall");
}

#[test]
fn a_slabs_perimeter_extension_still_takes_its_floor() {
    let mut rules = corner();
    rules.segments.floors.insert((1, 0, 0), FaceMaterials::uniform("floor"));
    let x = rules.geometry.cell_to_world_x(0);
    let z = rules.geometry.cell_to_world_z(0);
    let extension = Floor {
        x1: x - HALF,
        z1: z,
        x2: x,
        z2: z + CELL,
        y: LEVEL_HEIGHT,
        thickness: rules.geometry.floor_thickness(),
        level: 1,
        carrier: CarrierId::WORLD,
    };
    assert_eq!(rules.materials_for_floor(&extension).east, "floor");
}
