use bevy_math::Vec3;

use super::super::tests::test_map_layout;
use crate::{
    map::Carriers,
    physics::CollisionWorld,
    protocol::{
        Barrier, Carrier, CarrierId, FieldId, Position, PressurePlate, RampDirection, RampShape, SwitchId, SwitchState,
    },
    test_geometry::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS},
};

fn carriers(layout: &crate::protocol::MapLayout) -> Carriers {
    let mut carriers = Carriers::from_layout(layout);
    carriers.advance(0, &SwitchState::default());
    carriers
}

#[test]
fn walls_and_slabs_are_boxes_and_a_ramp_is_a_wedge() {
    let world = CollisionWorld::from_map_layout(&test_map_layout());
    let solids = world.structural_solids();
    assert_eq!(solids.len(), 3);

    let wall = &solids[0];
    assert!(wall.is_box());
    assert!(wall.min.distance(Vec3::new(0.0, LEVEL_HEIGHT, -WALL_THICKNESS / 2.0)) < 1e-5);
    assert!(
        wall.max
            .distance(Vec3::new(4.0, LEVEL_HEIGHT + WALL_HEIGHT, WALL_THICKNESS / 2.0))
            < 1e-5
    );

    let slab = &solids[1];
    assert!(slab.is_box());
    assert!((slab.min.y - (LEVEL_HEIGHT - FLOOR_THICKNESS)).abs() < 1e-5);
    assert!((slab.max.y - LEVEL_HEIGHT).abs() < 1e-5);

    let ramp = &solids[2];
    assert!(!ramp.is_box());
    assert_eq!(ramp.faces.len(), 5);
}

#[test]
fn shallow_planks_do_not_fill_their_bounding_boxes() {
    for direction in [
        RampDirection::North,
        RampDirection::South,
        RampDirection::East,
        RampDirection::West,
    ] {
        let mut layout = test_map_layout();
        layout.walls.clear();
        layout.floors.clear();
        let ramp = &mut layout.ramps[0];
        ramp.x2 = ramp.x1 + 100.0;
        ramp.z2 = ramp.z1 + 100.0;
        ramp.height = 2.0;
        ramp.direction = direction;
        ramp.shape = RampShape::Plank;
        let solids = CollisionWorld::from_map_layout(&layout).structural_solids();
        assert_eq!(solids.len(), 1);
        assert!(!solids[0].is_box(), "{direction:?} plank fills its bounds");
    }
}

// Shadows and footprints walk a face's edges, so its corners must lie in
// its plane and go round it one way.
#[test]
fn every_face_is_a_flat_convex_polygon_facing_outward() {
    let world = CollisionWorld::from_map_layout(&test_map_layout());
    for solid in world.structural_solids() {
        let corners: Vec<Vec3> = solid.faces.iter().flat_map(|face| face.corners.clone()).collect();
        let middle = corners.iter().sum::<Vec3>() / corners.len() as f32;
        for face in &solid.faces {
            assert!(face.corners.len() >= 3);
            assert!(face.normal.dot(face.corners[0] - middle) > 0.0, "{face:?} faces inward");
            let turns: Vec<f32> = (0..face.corners.len())
                .map(|index| {
                    let [a, b, c] = [0, 1, 2].map(|step| face.corners[(index + step) % face.corners.len()]);
                    assert!((face.normal.dot(b - a)).abs() < 1e-4, "{face:?} is not flat");
                    (b - a).cross(c - b).dot(face.normal)
                })
                .collect();
            assert!(
                turns.iter().all(|turn| *turn > 0.0) || turns.iter().all(|turn| *turn < 0.0),
                "{face:?} does not go round one way"
            );
        }
    }
}

#[test]
fn fields_and_plates_are_not_structure() {
    let mut layout = test_map_layout();
    layout.barriers.push(Barrier {
        x1: 0.0,
        z1: 2.0,
        x2: 4.0,
        z2: 2.0,
        width: 0.1,
        y: LEVEL_HEIGHT,
        height: WALL_HEIGHT,
        level: 1,
        levels: 1,
        field: FieldId(0),
        carrier: CarrierId::WORLD,
    });
    layout.pressure_plates.push(PressurePlate {
        level: 1,
        center_x: 2.0,
        center_y: LEVEL_HEIGHT,
        center_z: 2.0,
        side: 1.0,
        switch: SwitchId(0),
        carrier: CarrierId::WORLD,
    });
    assert_eq!(CollisionWorld::from_map_layout(&layout).structural_solids().len(), 3);
}

#[test]
fn nearby_solids_come_in_their_carriers_frame_where_it_stands_now() {
    let mut layout = test_map_layout();
    let parked = Position {
        x: 100.0,
        y: 0.0,
        z: 0.0,
    };
    layout.carriers.push(Carrier {
        motion: Default::default(),
        initially_on: true,
        parent: CarrierId::WORLD,
        level: 0,
        levels: 0,
        from: parked,
        to: parked,
        travel_ticks: 60,
        pause_ticks: 0,
        phase_ticks: 0,
        switch: None,
    });
    for wall in &mut layout.walls {
        wall.carrier = CarrierId(1);
    }
    let world = CollisionWorld::from_map_layout(&layout);
    let carriers = carriers(&layout);

    // The wall rides the carrier parked 100 m east; the slab and ramp stay.
    let near_wall = world.structural_solids_near(&carriers, Vec3::new(102.0, LEVEL_HEIGHT + 1.0, 0.0), 0.5);
    assert_eq!(near_wall.len(), 1);
    assert_eq!(near_wall[0].carrier, CarrierId(1));
    assert!(near_wall[0].min.x.abs() < 1e-4);
    assert!(near_wall[0].max.distance(world.structural_solids()[0].max) < 1e-4);

    assert!(
        world
            .structural_solids_near(&carriers, Vec3::new(2.0, LEVEL_HEIGHT + 1.0, 0.0), 0.5)
            .is_empty()
    );
    assert!(
        world
            .structural_solids_near(&carriers, Vec3::new(50.0, 50.0, 50.0), 1.0)
            .is_empty()
    );
}
