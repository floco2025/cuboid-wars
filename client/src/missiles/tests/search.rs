use super::*;
use crate::{
    constants::{MISSILE_SEARCH_TICK_QUERIES, MISSILE_SEARCH_WINDOW_MARGIN_CELLS},
    test_fixtures,
};
use common::protocol::{CarrierGrid, CarrierId, MapLayout, Wall};

#[test]
fn searches_resume_within_a_shared_budget_outside_authored_bounds() {
    let graph = AirGraph::new(
        &[CarrierGrid {
            carrier: CarrierId::WORLD,
            cols: 2,
            rows: 2,
            levels: 1,
        }],
        test_fixtures::sizes(),
    );
    let world = CollisionWorld::from_map_layout(&MapLayout {
        walls: vec![Wall {
            x1: 201.0,
            x2: 201.0,
            z1: -8.0,
            z2: 8.0,
            width: 0.3,
            y: 0.0,
            height: 8.0,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    });
    let carriers = Carriers::default();
    let start = Vec3::new(195.0, 1.0, 0.0);
    let goal = Vec3::new(205.0, 1.0, 0.0);
    let mut searches: Vec<_> = (0..8)
        .map(|_| {
            AirSearch::new(
                &graph,
                &carriers,
                &[],
                start,
                goal,
                0.3,
                0.0,
                MISSILE_SEARCH_WINDOW_MARGIN_CELLS,
            )
        })
        .collect();
    let mut paths = vec![None; searches.len()];
    let mut pending = false;
    for tick in 0..1000 {
        let mut budget = SearchBudget::new(32);
        for offset in 0..searches.len() {
            let index = (offset + tick) % searches.len();
            if paths[index].is_some() {
                continue;
            }
            match searches[index].advance(&graph, &carriers, &world, &mut budget) {
                SearchProgress::Pending => pending = true,
                SearchProgress::Found(path) => paths[index] = Some(path),
                _ => panic!("exterior target is reachable"),
            }
        }
        assert!(budget.used <= 32);
        if paths.iter().all(Option::is_some) {
            break;
        }
    }
    assert!(pending);
    for path in paths {
        let path = path.expect("every search gets a turn and completes");
        let mut previous = start;
        for point in path {
            assert!(sweep_clear(&world, &[], previous, point - previous, 0.3));
            previous = point;
        }
        assert_eq!(previous, goal);
    }
}

#[test]
fn sealed_target_searches_complete_under_the_shared_tick_budget() {
    measure_sealed_searches(false);
    measure_sealed_searches(true);
}

fn measure_sealed_searches(outside: bool) {
    use common::protocol::Floor;
    use std::time::Instant;
    let graph = AirGraph::new(
        &[CarrierGrid {
            carrier: CarrierId::WORLD,
            cols: 12,
            rows: 12,
            levels: 1,
        }],
        test_fixtures::sizes(),
    );
    let walls = [
        (-10.0, -10.0, 10.0, -10.0),
        (-10.0, 10.0, 10.0, 10.0),
        (-10.0, -10.0, -10.0, 10.0),
        (10.0, -10.0, 10.0, 10.0),
    ]
    .map(|(x1, z1, x2, z2)| Wall {
        x1,
        x2,
        z1,
        z2,
        y: 0.0,
        height: 4.0,
        width: 0.3,
        level: 0,
        carrier: CarrierId::WORLD,
    });
    let world = CollisionWorld::from_map_layout(&MapLayout {
        walls: walls.to_vec(),
        floors: [0.0, 4.4]
            .map(|y| Floor {
                x1: -10.0,
                x2: 10.0,
                z1: -10.0,
                z2: 10.0,
                y,
                thickness: 0.4,
                level: 0,
                carrier: CarrierId::WORLD,
            })
            .to_vec(),
        ..Default::default()
    });
    let carriers = Carriers::default();
    let inside = Vec3::new(0.0, 1.0, 0.0);
    let exterior = Vec3::new(20.0, 1.0, 0.0);
    let (from, to) = if outside {
        (exterior, inside)
    } else {
        (inside, exterior)
    };
    let mut searches: Vec<_> = (0..16)
        .map(|_| {
            Some(AirSearch::new(
                &graph,
                &carriers,
                &[],
                from,
                to,
                0.3,
                1.0,
                MISSILE_SEARCH_WINDOW_MARGIN_CELLS,
            ))
        })
        .collect();
    let mut total = 0;
    let mut slowest = std::time::Duration::ZERO;
    for tick in 0..10000 {
        let start = Instant::now();
        let mut budget = SearchBudget::new(MISSILE_SEARCH_TICK_QUERIES);
        for offset in 0..searches.len() {
            let index = (tick + offset) % searches.len();
            let Some(search) = &mut searches[index] else {
                continue;
            };
            match search.advance(&graph, &carriers, &world, &mut budget) {
                SearchProgress::Unreachable if !outside => searches[index] = None,
                SearchProgress::WindowLimited | SearchProgress::NodeLimited if outside => searches[index] = None,
                SearchProgress::Pending => {}
                _ => panic!("sealed room must exhaust its connected airspace"),
            }
        }
        slowest = slowest.max(start.elapsed());
        total += budget.used;
        assert!(budget.used <= MISSILE_SEARCH_TICK_QUERIES);
        if searches.iter().all(Option::is_none) {
            break;
        }
    }
    assert!(searches.iter().all(Option::is_none));
    eprintln!(
        "16 sealed-room searches (outside={outside}): {total} reserved collision queries total, slowest shared tick {slowest:?}"
    );
}
