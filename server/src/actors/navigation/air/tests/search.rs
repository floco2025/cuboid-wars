use super::*;
use crate::actors::test_kinds::{self, CONTACT};
use common::protocol::{CarrierId, Floor, MapLayout, Wall};

#[test]
fn route_goes_above_a_wall_without_floor_and_respects_work_budget() {
    let physics = test_kinds::physics(CONTACT);
    let world = CollisionWorld::from_map_layout(&MapLayout {
        walls: vec![Wall {
            x1: 0.0,
            z1: -12.0,
            x2: 0.0,
            z2: 12.0,
            y: -10.0,
            height: 12.0,
            width: 0.3,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    });
    let start = Position::from(Vec3::new(-3.0, 0.0, 0.0));
    let target = Position::from(Vec3::new(3.0, 0.0, 0.0));
    let mut search = AirSearch::new(start, target, physics.movement_collider.radius());
    let mut budget = 1;
    assert!(matches!(
        search.advance(&world, physics, &[], &mut budget, |_| true),
        SearchResult::Pending
    ));
    assert_eq!(budget, 0);
    for _ in 0..100 {
        let mut budget = 128;
        if let SearchResult::Found(path) = search.advance(&world, physics, &[], &mut budget, |_| true) {
            assert!(path.iter().any(|p| p.y > 2.0));
            let mut previous = start;
            for point in path {
                assert!(world.character_flight_path_clear(previous, point, physics, &[]));
                previous = point;
            }
            assert_eq!(previous, target);
            return;
        }
    }
    panic!("air route missing above wall");
}

#[test]
fn direct_flight_has_no_map_or_altitude_boundary() {
    let physics = test_kinds::physics(CONTACT);
    let world = CollisionWorld::from_map_layout(&MapLayout::default());
    let start = Position::default();
    let target = Position::from(Vec3::new(10000.0, -900.0, 20000.0));
    let mut search = AirSearch::new(start, target, physics.movement_collider.radius());
    let SearchResult::Found(path) = search.advance(&world, physics, &[], &mut 1, |_| true) else {
        panic!("direct air route missing");
    };
    assert_eq!(path, VecDeque::from([target]));
}

#[test]
fn escape_finds_alternative_cover_around_a_wall_with_a_small_tick_budget() {
    let physics = test_kinds::physics(CONTACT);
    let world = CollisionWorld::from_map_layout(&MapLayout {
        walls: vec![Wall {
            x1: 0.0,
            z1: -1.0,
            x2: 0.0,
            z2: 1.0,
            y: -10.0,
            height: 20.0,
            width: 0.3,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        floors: vec![Floor {
            x1: -30.0,
            z1: -30.0,
            x2: 30.0,
            z2: 30.0,
            y: 0.0,
            thickness: 0.4,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    });
    let start = Position::from(Vec3::new(-1.5, 1.0, 0.0));
    let target = Position::from(Vec3::new(1.5, -3.0, 0.0));
    let mut search = AirSearch::new(start, target, physics.movement_collider.radius());
    let covered = |p: Vec3| p.x > 0.8 && p.z.abs() < 1.0;
    for _ in 0..16 {
        let mut budget = 16;
        let result = search.advance_escape(&world, physics, &[], &mut budget, covered, |p| {
            p.distance_squared(Vec3::new(-4.0, 0.0, 0.0))
        });
        if let SearchResult::Found(route) = result {
            let end = *route.back().expect("escape endpoint missing");
            assert!(covered(Vec3::from(end)));
            assert!(end.y > 0.0);
            assert!(route.iter().any(|p| p.z.abs() > 1.0));
            let mut previous = start;
            for point in route {
                assert!(world.character_flight_path_clear(previous, point, physics, &[]));
                previous = point;
            }
            return;
        }
        assert!(matches!(result, SearchResult::Pending));
        assert_eq!(budget, 0);
    }
    panic!("reachable escape route missing around wall");
}
