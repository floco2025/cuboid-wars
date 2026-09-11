use super::*;

// The scuttler-in-the-trench jam: the actor overshot the first waypoint along
// the next leg (a ramp-top transition sits at the actor's own cell centre)
// and must not be sent back for it.
#[test]
fn overshot_waypoint_on_the_next_leg_is_skipped() {
    let fixture = Fixture::new(CONTACT);
    let first = fixture.pos(2, 2);
    let second = fixture.pos(5, 2);
    let mut info = info(CONTACT);
    info.route = Some(route_through(&[first, second], &fixture));
    let overshot = Position {
        x: first.x + 0.9,
        ..first
    };

    tick_route(&mut info, overshot, &fixture);

    assert_eq!(
        info.route
            .as_ref()
            .map(|route| route.waypoints.front().map(|point| point.position)),
        Some(Some(second))
    );
}

#[test]
fn waypoint_ahead_on_the_next_leg_is_kept() {
    let fixture = Fixture::new(CONTACT);
    let first = fixture.pos(2, 2);
    let second = fixture.pos(5, 2);
    let mut info = info(CONTACT);
    info.route = Some(route_through(&[first, second], &fixture));
    let approaching = Position {
        x: first.x - 0.9,
        ..first
    };

    tick_route(&mut info, approaching, &fixture);

    assert_eq!(
        info.route
            .as_ref()
            .map(|route| route.waypoints.front().map(|point| point.position)),
        Some(Some(first))
    );
}

#[test]
fn corner_waypoint_is_not_skipped_from_the_side() {
    let fixture = Fixture::new(CONTACT);
    let corner = fixture.pos(2, 2);
    let after = fixture.pos(5, 2);
    let mut info = info(CONTACT);
    info.route = Some(route_through(&[corner, after], &fixture));
    // Approaching the corner along the row axis: beyond it along the next
    // leg by a hair, but a full cell off that leg's line.
    let beside = Position {
        x: corner.x + 0.1,
        z: corner.z + 3.0,
        ..corner
    };

    tick_route(&mut info, beside, &fixture);

    assert_eq!(
        info.route
            .as_ref()
            .map(|route| route.waypoints.front().map(|point| point.position)),
        Some(Some(corner))
    );
}

#[test]
fn final_waypoint_is_only_dropped_when_reached() {
    let fixture = Fixture::new(CONTACT);
    let only = fixture.pos(2, 2);
    let mut info = info(CONTACT);
    info.route = Some(route_through(&[only], &fixture));
    let past = Position {
        x: only.x + 0.9,
        ..only
    };

    tick_route(&mut info, past, &fixture);

    assert!(info.route.is_some());
}

#[test]
fn stalled_actor_hops_to_a_random_neighbor_before_rethinking() {
    let fixture = Fixture::new(CONTACT);
    let mut info = info(CONTACT);
    let pos = fixture.pos(5, 2);
    info.set_route(Some(route_through(&[fixture.pos(9, 2)], &fixture)));

    let mut stalled = false;
    for _ in 0..20 {
        stalled = tick_runtime_state(&mut info, pos, 0.1, fixture.server.expect_actor(CONTACT), &[]);
        if stalled {
            break;
        }
    }
    assert!(stalled, "pinned actor must trip the watchdog");

    let mut rng = StdRng::seed_from_u64(1);
    shake_loose(&mut info, &fixture.context(CONTACT, pos), &mut rng);

    let route = info.route.as_ref().expect("shake installs a hop route");
    assert_eq!(route.waypoints.len(), 1, "one-leg hop");
    let hop_distance = pos.horizontal_distance_sq(&route.destination).sqrt();
    assert!(
        hop_distance > 0.1 && hop_distance < CELL * 1.6,
        "hop lands in a neighboring cell, got {hop_distance}"
    );
    assert!(info.decision_timer > 0.0, "controller deferred during the hop");
}

#[test]
fn climbing_progress_uses_height_and_does_not_skip_to_the_exit() {
    let fixture = Fixture::new(CONTACT);
    let mut info = info(CONTACT);
    let bottom = fixture.pos(2, 2);
    let top = Position {
        y: LEVEL_HEIGHT,
        ..bottom
    };
    let exit = Position { x: top.x + CELL, ..top };
    let mut route = route_through(&[top, exit], &fixture);
    route.waypoints[0].kind = WaypointKind::Climb {
        normal_x: 0.0,
        normal_z: -1.0,
        ascending: true,
    };
    route.waypoints[1].kind = WaypointKind::Exit;
    info.set_route(Some(route));
    for tick in 0..30 {
        let pos = Position {
            y: tick as f32 * 0.1,
            ..bottom
        };
        assert!(!tick_runtime_state(
            &mut info,
            pos,
            0.1,
            fixture.server.expect_actor(CONTACT),
            &[]
        ));
        assert_eq!(info.route.as_ref().expect("ladder route missing").waypoints.len(), 2);
    }
    tick_route(&mut info, top, &fixture);
    assert_eq!(info.route.as_ref().expect("ladder route missing").waypoints.len(), 1);
    tick_route(&mut info, Position { y: 0.0, ..exit }, &fixture);
    assert!(
        info.route.is_some(),
        "horizontal proximity cannot finish a ladder exit at another height"
    );
    tick_route(&mut info, exit, &fixture);
    assert!(info.route.is_none());
}

#[test]
fn reached_ladder_target_remains_a_hold_instead_of_becoming_idle() {
    let fixture = Fixture::new(CONTACT);
    let mut info = info(CONTACT);
    let target = Position {
        y: 2.0,
        ..fixture.pos(2, 2)
    };
    let mut route = route_through(&[target], &fixture);
    route.waypoints[0].kind = WaypointKind::Climb {
        normal_x: 0.0,
        normal_z: -1.0,
        ascending: true,
    };
    info.set_route(Some(route));
    for _ in 0..30 {
        assert!(!tick_runtime_state(
            &mut info,
            target,
            0.1,
            fixture.server.expect_actor(CONTACT),
            &[]
        ));
    }
    assert!(info.route.is_some());
}

// The fixture's grid with cell (3, 2) a bridge instead of floor.
fn graph_with_bridge_at_3_2(fixture: &Fixture) -> NavGraph {
    let cols = 12;
    let rows = 5;
    let mut cells = CellGrid::new(cols, rows);
    for row in &mut cells.rows {
        for cell in row {
            cell.has_floor = true;
        }
    }
    cells.rows[2][3].has_floor = false;
    cells.rows[2][3].bridge = Some(BridgeId(0));
    NavGraph::new(&CarrierGrid::new(
        CarrierId::WORLD,
        fixture.geometry,
        vec![LevelGrid {
            cells,
            edges: EdgeGrid::new(cols, rows),
            barrier_edges: EdgeGrid::new(cols, rows),
        }],
    ))
}

#[test]
fn route_onto_a_bridge_that_lost_power_is_dropped_for_a_fresh_decision() {
    let fixture = Fixture::new(CONTACT);
    let mut graph = graph_with_bridge_at_3_2(&fixture);
    let mut info = info(CONTACT);
    info.route = Some(route_through(&[fixture.pos(3, 2), fixture.pos(5, 2)], &fixture));
    info.decision_timer = 1.0;

    drop_route_onto_lost_bridge(&mut info, &graph);
    assert!(info.route.is_some(), "a powered bridge keeps the route");
    assert_eq!(info.decision_timer, 1.0);

    graph.set_powered_bridges(&[]);
    drop_route_onto_lost_bridge(&mut info, &graph);
    assert!(info.route.is_none(), "the leg onto the gap is abandoned");
    assert_eq!(info.decision_timer, 0.0, "the actor decides afresh at once");
}

#[test]
fn route_over_floor_survives_a_bridge_losing_power_elsewhere() {
    let fixture = Fixture::new(CONTACT);
    let mut graph = graph_with_bridge_at_3_2(&fixture);
    let mut info = info(CONTACT);
    info.route = Some(route_through(&[fixture.pos(2, 2), fixture.pos(2, 4)], &fixture));

    graph.set_powered_bridges(&[]);
    drop_route_onto_lost_bridge(&mut info, &graph);

    assert!(info.route.is_some());
}
