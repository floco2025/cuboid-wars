use std::f32::consts::{FRAC_PI_2, TAU};

use super::{
    super::test_fixtures::{game_physics, physics_with},
    *,
};

const FLOOR: [f32; 3] = [0.0, 1.0, 0.0];

fn request(point: [f32; 3], jumping: bool, heights: &[f32]) -> JumpRequest {
    JumpRequest {
        takeoff: TakeoffSpec {
            point,
            direction: [0.0, 1.0],
            jumping,
            margin: 0.0,
        },
        heights: heights.to_vec(),
        air_control: false,
        shooter: [point[0], point[2]],
        portals: None,
    }
}

fn floor_portal(x: f32, y: f32, z: f32) -> SurfaceSpec {
    SurfaceSpec {
        center: [x, y, z],
        normal: FLOOR,
        yaw: 0.0,
    }
}

fn wall_portal(center: [f32; 3], normal: [f32; 3]) -> SurfaceSpec {
    SurfaceSpec {
        center,
        normal,
        yaw: 0.0,
    }
}

fn through(request: &JumpRequest, entry: SurfaceSpec, exit: Option<SurfaceSpec>) -> JumpRequest {
    JumpRequest {
        portals: Some(PortalsSpec { entry, exit }),
        ..request.clone()
    }
}

fn preview(physics: &PreviewPhysics, request: &JumpRequest) -> Vec<ScenarioPreview> {
    jump_preview(physics, request).expect("request is valid").scenarios
}

fn crossing(scenario: &ScenarioPreview, level: usize, phase: Phase) -> &CrossingPreview {
    scenario
        .crossings
        .iter()
        .find(|crossing| crossing.level == level && crossing.phase == phase)
        .unwrap_or_else(|| panic!("no {phase:?} crossing of level {level}: {:?}", scenario.crossings))
}

fn capture_on(scenario: &ScenarioPreview, level: usize) -> &[PiecePreview] {
    &scenario
        .capture
        .iter()
        .find(|capture| capture.level == level)
        .unwrap_or_else(|| panic!("no capture on level {level}"))
        .pieces
}

fn bounds(polygon: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    polygon
        .iter()
        .fold(([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]), |(low, high), point| {
            (
                [low[0].min(point[0]), low[1].min(point[1])],
                [high[0].max(point[0]), high[1].max(point[1])],
            )
        })
}

// How far inside a convex polygon a point is; negative outside.
fn depth(polygon: &[[f64; 2]], point: [f64; 2]) -> f64 {
    let turn: f64 = (0..polygon.len())
        .map(|index| {
            let (a, b) = (polygon[index], polygon[(index + 1) % polygon.len()]);
            a[0] * b[1] - a[1] * b[0]
        })
        .sum();
    (0..polygon.len())
        .map(|index| {
            let (a, b) = (polygon[index], polygon[(index + 1) % polygon.len()]);
            let (edge, to) = ([b[0] - a[0], b[1] - a[1]], [point[0] - a[0], point[1] - a[1]]);
            (edge[0] * to[1] - edge[1] * to[0]) * turn.signum() / edge[0].hypot(edge[1])
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn a_jump_comes_down_on_its_own_level_where_each_scenario_carries_it() {
    // No air braking: speed 1 and jump 2 at gravity 2 are airborne 2 s.
    let scenarios = preview(&physics_with(0.0), &request([0.0; 3], true, &[0.0]));
    let expected = [(2.0, 2.0), (2.0, 4.0), (4.0, 4.0), (4.0, 8.0)];
    assert_eq!(scenarios.len(), 4);
    for (scenario, (time, reach)) in scenarios.iter().zip(expected) {
        let landing = crossing(scenario, 0, Phase::BeforeEntry);
        assert!((landing.time - time).abs() < 2e-3, "{landing:?}");
        assert!((landing.point[1] - reach).abs() < 2e-3, "{landing:?}");
        assert_eq!(landing.point[0], 0.0);
        assert_eq!(scenario.path[0], [0.0; 3]);
        assert_eq!(scenario.end, End::Below);
        assert!(scenario.hop.is_none() && scenario.entry.is_none());
    }
}

#[test]
fn a_braked_jump_returns_to_its_takeoff_floor_but_cannot_land_above_a_ceiling() {
    let mut physics = game_physics();
    physics.player.move_speed = 1.0;
    physics.player.air_deceleration = 60.0;
    let mut jump = request([0.0; 3], true, &[0.0, 0.5]);
    jump.takeoff.margin = 0.1;
    for scenario in preview(&physics, &jump) {
        let landing = crossing(&scenario, 0, Phase::BeforeEntry);
        assert_eq!(landing.point, [scenario.path[0][0], scenario.path[0][2]]);
        assert!(scenario.crossings.iter().all(|crossing| crossing.level == 0));
        assert!(!capture_on(&scenario, 0).is_empty());
    }
}

#[test]
fn a_step_leaves_from_the_edge_and_a_jump_from_the_margin_before_it() {
    let physics = physics_with(0.0);
    let step = &preview(&physics, &request([0.0, 5.0, 0.0], false, &[0.0, 5.0]))[0];
    assert_eq!(step.crossings.len(), 1, "{:?}", step.crossings);
    let landing = crossing(step, 0, Phase::BeforeEntry);
    assert!((landing.time - 5.0_f64.sqrt()).abs() < 2e-3, "{landing:?}");
    assert!((landing.point[1] - 5.0_f64.sqrt()).abs() < 2e-3, "{landing:?}");

    let mut early = request([0.0, 5.0, 0.0], true, &[0.0, 5.0]);
    early.takeoff.margin = 0.5;
    let jump = &preview(&physics, &early)[0];
    assert_eq!(jump.path[0], [0.0, 5.0, -0.5]);
    assert!((crossing(jump, 1, Phase::BeforeEntry).point[1] - 1.5).abs() < 2e-3);
    assert!(crossing(jump, 0, Phase::BeforeEntry).time > 2.0);
}

#[test]
fn damage_uses_the_normal_gravity_drop() {
    let physics = physics_with(0.0);
    let fall = |height: f32| preview(&physics, &request([0.0, height, 0.0], false, &[0.0]));
    let lethal = fall(13.0);
    assert_eq!(crossing(&lethal[0], 0, Phase::BeforeEntry).damage, 1.0);
    // Half gravity lands like a 6.5 m drop, between safe 4 and lethal 12.
    assert!((crossing(&lethal[2], 0, Phase::BeforeEntry).damage - 0.3125).abs() < 0.02);
    assert_eq!(crossing(&fall(3.0)[0], 0, Phase::BeforeEntry).damage, 0.0);
}

#[test]
fn near_lethal_fall_damage_keeps_its_survival_classification() {
    let mut physics = physics_with(0.0);
    physics.player_fall.lethal_distance = 9.9235;
    let fall = request([0.0, 10.0, 0.0], false, &[0.0]);
    let scenarios = preview(&physics, &fall);
    let damage = crossing(&scenarios[0], 0, Phase::BeforeEntry).damage;
    assert!((0.9995..1.0).contains(&damage), "{damage}");
    physics.player_fall.lethal_distance = 9.92;
    assert_eq!(
        crossing(&preview(&physics, &fall)[0], 0, Phase::BeforeEntry).damage,
        1.0
    );
}

#[test]
fn a_flight_gravity_never_brings_down_ends_at_the_time_cap() {
    let mut physics = physics_with(0.0);
    physics.low_gravity = 0.0;
    let scenarios = preview(&physics, &request([0.0, 5.0, 0.0], false, &[0.0]));
    assert_eq!(scenarios[0].end, End::Below);
    assert_eq!(scenarios[2].end, End::TimeCap);
    assert!(scenarios[2].crossings.is_empty());
    assert!((scenarios[2].path.len() as f32 - PREVIEW_MAX_SECS * 30.0).abs() <= 2.0);
}

#[test]
fn capture_is_the_margin_rectangle_about_the_landing_split_at_the_quarter_wedges() {
    let physics = physics_with(0.0);
    let mut step = request([0.0, 5.0, 0.0], false, &[0.0, 5.0]);
    // Shot from far behind, every centre near the landing faces the same way.
    step.shooter = [0.0, -50.0];
    let scenario = &preview(&physics, &step)[0];
    let landing = crossing(scenario, 0, Phase::BeforeEntry).point;
    assert_eq!(scenario.capture.len(), 1);
    let pieces = capture_on(scenario, 0);
    assert_eq!(pieces.len(), 1, "{pieces:?}");
    assert_eq!(pieces[0].yaw, 0.0);
    let (low, high) = bounds(&pieces[0].polygon);
    // Width 1.4 and height 2.6 with a 0.6 margin, the long axis away from the shooter.
    assert!(
        (low[0] + 1.3).abs() < 1e-3 && (high[0] - 1.3).abs() < 1e-3,
        "{low:?} {high:?}"
    );
    assert!(high[1] > landing[1] + 1.8 && high[1] <= landing[1] + 1.9, "{high:?}");
    assert!(low[1] > landing[1] - 1.9 && low[1] < landing[1] - 1.8, "{low:?}");

    // Shot from the takeoff, centres beside the landing face sideways and lie long that way.
    step.shooter = [0.0, 0.0];
    let scenario = &preview(&physics, &step)[0];
    let pieces = capture_on(scenario, 0);
    assert_eq!(pieces.len(), 3, "{pieces:?}");
    let reach = |yaw: f32| {
        let piece = pieces
            .iter()
            .find(|piece| (piece.yaw - f64::from(yaw)).abs() < 1e-3)
            .unwrap_or_else(|| panic!("no piece at yaw {yaw}"));
        bounds(&piece.polygon)
    };
    assert!((reach(FRAC_PI_2).1[0] - 1.9).abs() < 1e-3);
    assert!((reach(3.0 * FRAC_PI_2).0[0] + 1.9).abs() < 1e-3);
    assert!(reach(0.0).1[0] <= 1.3 + 1e-3);
}

#[test]
fn without_a_funnel_capture_is_the_aperture_the_body_sinks_through() {
    let mut physics = physics_with(0.0);
    physics.funnel.capture_margin = 0.0;
    let mut step = request([0.0, 5.0, 0.0], false, &[0.0, 5.0]);
    step.shooter = [0.0, -50.0];
    let scenario = &preview(&physics, &step)[0];
    let landing = crossing(scenario, 0, Phase::BeforeEntry).point;
    let (low, high) = bounds(&capture_on(scenario, 0)[0].polygon);
    assert!((high[0] - 0.7).abs() < 1e-3 && (low[0] + 0.7).abs() < 1e-3);
    assert!(high[1] - low[1] < 2.6 && high[1] - low[1] > 2.0, "{low:?} {high:?}");
    assert!(low[1] < landing[1] && high[1] > landing[1]);
}

// The regions are the simulation's own answer: a portal placed anywhere
// around the landing is entered with released input exactly when its centre
// lies in a capture piece.
#[test]
fn capture_pieces_agree_with_flying_into_a_portal_at_every_centre() {
    let (mut inside, mut outside) = (0, 0);
    for (margin, jumping, shooter) in [
        (0.6, true, [0.0, 0.0]),
        (0.6, false, [3.0, 4.0]),
        (0.0, true, [0.0, 0.0]),
        (0.0, false, [-2.0, 9.0]),
    ] {
        let mut physics = game_physics();
        physics.funnel.capture_margin = margin;
        let mut takeoff = request([0.0, 8.0, 0.0], jumping, &[0.0, 8.0]);
        takeoff.takeoff.margin = 0.1;
        takeoff.shooter = shooter;
        let free = preview(&physics, &takeoff);
        let landing = crossing(&free[0], 0, Phase::BeforeEntry).point;
        for column in -20..=20 {
            for row in -20..=44 {
                let centre = [
                    (landing[0] + f64::from(column) * 0.125) as f32,
                    (landing[1] + f64::from(row) * 0.125) as f32,
                ];
                let entry = SurfaceSpec {
                    center: [centre[0], 0.0, centre[1]],
                    normal: FLOOR,
                    yaw: (centre[0] - shooter[0]).atan2(centre[1] - shooter[1]),
                };
                let flown = preview(&physics, &through(&takeoff, entry, None));
                for (free, flown) in free.iter().zip(&flown) {
                    let depths: Vec<f64> = capture_on(free, 0)
                        .iter()
                        .map(|piece| depth(&piece.polygon, [f64::from(centre[0]), f64::from(centre[1])]))
                        .collect();
                    if depths.iter().any(|depth| depth.abs() < 0.03) {
                        continue;
                    }
                    let expected = depths.iter().any(|depth| *depth > 0.0);
                    let entered = matches!(flown.entry, Some(Entry::Direct | Entry::Funnel));
                    assert_eq!(
                        entered, expected,
                        "margin {margin}, jumping {jumping}, centre {centre:?}: {:?}",
                        flown.entry
                    );
                    if entered {
                        inside += 1;
                    } else {
                        outside += 1;
                    }
                }
            }
        }
    }
    assert!(inside > 2000 && outside > 10000, "{inside} in, {outside} out");
}

#[test]
fn steering_range_holds_the_released_landing_and_collapses_without_air_rates() {
    let mut steered = request([0.0, 8.0, 0.0], true, &[0.0, 8.0]);
    steered.air_control = true;
    let scenario = &preview(&game_physics(), &steered)[0];
    let landing = crossing(scenario, 0, Phase::BeforeEntry).point;
    let range = &scenario
        .range
        .iter()
        .find(|range| range.level == 0)
        .expect("level 0 is crossed")
        .polygon;
    assert!(range.len() >= 6, "{range:?}");
    assert!(depth(range, landing) >= -1e-3, "{landing:?} outside {range:?}");
    // Held forward keeps the takeoff speed released input brakes away.
    assert!(bounds(range).1[1] > landing[1] + 1.0);
    let caught = &scenario
        .capture_steered
        .iter()
        .find(|level| level.level == 0)
        .expect("level 0")
        .pieces;
    let forward = caught
        .iter()
        .filter(|piece| piece.yaw == 0.0)
        .map(|piece| bounds(&piece.polygon).1[1])
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(forward > bounds(range).1[1] + 1.0);

    let mut ballistic = game_physics();
    ballistic.player.air_acceleration = 0.0;
    ballistic.player.air_deceleration = 0.0;
    ballistic.player.air_lateral_deceleration = 0.0;
    let scenario = &preview(&ballistic, &steered)[0];
    let landing = crossing(scenario, 0, Phase::BeforeEntry).point;
    let range = &scenario
        .range
        .iter()
        .find(|range| range.level == 0)
        .expect("level 0 is crossed")
        .polygon;
    assert_eq!(range, &vec![landing]);
    for margin in [0.0, 0.6] {
        ballistic.funnel.capture_margin = margin;
        for mut scenario in preview(&ballistic, &steered) {
            scenario.capture.sort_by_key(|capture| capture.level);
            scenario.capture_steered.sort_by_key(|capture| capture.level);
            assert_eq!(
                serde_json::to_value(&scenario.capture).expect("capture serialization failed"),
                serde_json::to_value(&scenario.capture_steered).expect("steered capture serialization failed"),
            );
        }
    }

    steered.air_control = false;
    let scenario = &preview(&game_physics(), &steered)[0];
    assert!(scenario.range.is_empty() && scenario.capture_steered.is_empty());
}

#[test]
fn every_steered_capture_piece_has_a_flight_that_enters_its_portals_after_release() {
    for margin in [0.0, 0.6] {
        let mut physics = game_physics();
        physics.funnel.capture_margin = margin;
        let mut jump = request([0.0, 8.0, 0.0], true, &[0.0]);
        jump.air_control = true;
        let air = Air {
            physics: &physics,
            scenario: SCENARIOS[0],
            heights: &jump.heights,
        };
        let origin = Origin {
            state: PlayerFlightState {
                position: Vec3::new(0.0, 8.0, 0.0),
                horizontal_velocity: Vec3::Z * air.speed(),
                vertical_velocity: physics.player.jump_speed,
            },
            time: 0.0,
            phase: Phase::BeforeEntry,
        };
        let wishes = (0..PREVIEW_STEERING_DIRECTIONS).map(|index| {
            let (sin, cos) = (index as f32 * TAU / PREVIEW_STEERING_DIRECTIONS as f32).sin_cos();
            Steering::Constant(Vec3::new(sin, 0.0, cos) * air.speed())
        });
        let releases: Vec<_> = wishes
            .chain([Steering::Released])
            .flat_map(|steering| {
                air.fly(origin, steering, None)
                    .crossings
                    .into_iter()
                    .map(|crossing| crossing.from)
            })
            .collect();
        let scenarios = preview(&physics, &jump);
        let pieces = &scenarios[0].capture_steered[0].pieces;
        assert!(!pieces.is_empty());
        for piece in pieces {
            let points: Vec<_> = piece
                .polygon
                .iter()
                .map(|point| Vec2::new(point[0] as f32, point[1] as f32))
                .collect();
            let middle = points.iter().sum::<Vec2>() / points.len() as f32;
            for point in points.iter().map(|point| middle.lerp(*point, 0.9)) {
                let entry = SurfaceSpec {
                    yaw: point.x.atan2(point.y),
                    ..floor_portal(point.x, 0.0, point.y)
                };
                let pair = Pair::new(&physics, &PortalsSpec { entry, exit: None });
                let gates = pair.gates(&physics, physics.funnel);
                assert!(
                    releases.iter().any(|&release| {
                        air.fly(release, Steering::Released, Some(&gates)).outcome == Outcome::Entered
                    }),
                    "uncapturable portal at {point:?}, margin {margin}"
                );
            }
        }
    }
}

#[test]
fn an_entry_is_direct_funnelled_steered_or_missed() {
    let physics = game_physics();
    let jump = request([0.0, 8.0, 0.0], true, &[0.0, 8.0]);
    let free = &preview(&physics, &jump)[0];
    let landing = crossing(free, 0, Phase::BeforeEntry).point.map(|value| value as f32);
    let entry_at = |x: f32, z: f32, air_control: bool| {
        let mut request = through(&jump, floor_portal(landing[0] + x, 0.0, landing[1] + z), None);
        request.air_control = air_control;
        preview(&physics, &request).swap_remove(0)
    };
    assert_eq!(entry_at(0.0, 0.0, false).entry, Some(Entry::Direct));
    assert_eq!(entry_at(1.0, 0.0, false).entry, Some(Entry::Funnel));

    // Just past the far edge of the capture region the body lands in front of the portal.
    let missed = entry_at(0.0, 2.2, false);
    assert_eq!(missed.entry, Some(Entry::Missed));
    assert!(missed.hop.is_none());
    assert_eq!(missed.path, free.path);
    assert_eq!(
        crossing(&missed, 0, Phase::BeforeEntry).point,
        crossing(free, 0, Phase::BeforeEntry).point
    );

    let steered = entry_at(0.0, 2.2, true);
    assert_eq!(steered.entry, Some(Entry::Steered));
    assert!(steered.hop.is_some());
    assert_ne!(steered.path, free.path);
    assert_eq!(entry_at(0.0, 30.0, true).entry, Some(Entry::Missed));
}

#[test]
fn an_entry_without_an_exit_ends_the_path_inside_the_portal() {
    let physics = game_physics();
    let jump = request([0.0, 8.0, 0.0], true, &[0.0, 8.0]);
    let landing = crossing(&preview(&physics, &jump)[0], 0, Phase::BeforeEntry)
        .point
        .map(|value| value as f32);
    let entered = &preview(
        &physics,
        &through(&jump, floor_portal(landing[0], 0.0, landing[1]), None),
    )[0];
    assert_eq!(entered.end, End::Entered);
    assert_eq!(entered.hop, Some(entered.path.len() - 1));
    assert!(entered.hop_time.is_some());
    // Sinking into the portal is not a landing on its floor.
    assert!(
        entered.crossings.iter().all(|crossing| crossing.level == 1),
        "{:?}",
        entered.crossings
    );
}

#[test]
fn a_fall_into_a_floor_portal_leaves_a_wall_portal_at_the_fall_speed() {
    let physics = game_physics();
    let step = request([0.0, 10.0, 0.0], false, &[0.0, 10.0]);
    let landing = crossing(&preview(&physics, &step)[0], 0, Phase::BeforeEntry)
        .point
        .map(|value| value as f32);
    let exit = wall_portal([30.0, 1.3, 30.0], [1.0, 0.0, 0.0]);
    let flown = &preview(
        &physics,
        &through(&step, floor_portal(landing[0], 0.0, landing[1]), Some(exit)),
    )[0];
    let hop = flown.hop.expect("the flight enters");
    let out = flown.path[hop + 1];
    assert!((out[0] - 30.0).abs() < 1.0 && (out[2] - 30.0).abs() < 0.7, "{out:?}");
    // About 14 m/s out of the wall: far more than the move speed, bled only by air braking.
    let next = flown.path[hop + 2];
    assert!((next[0] - out[0]) * 30.0 > 12.0, "{out:?} -> {next:?}");
    let landed = crossing(flown, 0, Phase::AfterExit);
    assert!(landed.point[0] > 32.0, "{landed:?}");
    assert!(
        flown
            .crossings
            .iter()
            .all(|crossing| crossing.level != 0 || crossing.phase == Phase::AfterExit)
    );
    assert_eq!(flown.end, End::Below);

    let mut steered = through(&step, floor_portal(landing[0], 0.0, landing[1]), Some(exit));
    steered.air_control = true;
    let flown = &preview(&physics, &steered)[0];
    let range = &flown
        .exit_range
        .iter()
        .find(|range| range.level == 0)
        .expect("level 0")
        .polygon;
    assert!(depth(range, crossing(flown, 0, Phase::AfterExit).point) >= -1e-3);
}

#[test]
fn a_walk_into_a_wall_portal_stands_on_the_floor_of_the_wall_it_leaves() {
    let physics = physics_with(0.0);
    let step = request([0.0; 3], false, &[0.0, 5.0]);
    let entry = wall_portal([0.0, 1.3, 0.5], [0.0, 0.0, -1.0]);
    let exit = wall_portal([10.0, 6.3, 10.0], [1.0, 0.0, 0.0]);
    let flown = &preview(&physics, &through(&step, entry, Some(exit)))[0];
    assert_eq!(flown.entry, Some(Entry::Direct));
    let stood = crossing(flown, 1, Phase::AfterExit);
    assert!(
        (stood.point[0] - 10.0).abs() < 0.5 && (stood.point[1] - 10.0).abs() < 0.1,
        "{stood:?}"
    );
    assert_eq!(stood.time, flown.hop_time.expect("the flight enters"));
}

#[test]
fn a_wall_entry_leaves_a_floor_portal_upward() {
    let physics = game_physics();
    let step = request([0.0; 3], false, &[0.0]);
    let entry = wall_portal([0.0, 1.3, 1.0], [0.0, 0.0, -1.0]);
    let flown = &preview(&physics, &through(&step, entry, Some(floor_portal(20.0, 0.0, 20.0))))[0];
    let hop = flown.hop.expect("the flight enters");
    let (out, next) = (flown.path[hop + 1], flown.path[hop + 2]);
    assert!((out[0] - 20.0).abs() < 0.7 && (out[2] - 20.0).abs() < 1.3, "{out:?}");
    assert!(next[1] > out[1], "{out:?} -> {next:?}");
}

// Ballistic, as in a map with no air rates: nothing but the portals changes the flight.
fn ballistic_physics() -> PreviewPhysics {
    let mut physics = game_physics();
    physics.player.air_acceleration = 0.0;
    physics.player.air_deceleration = 0.0;
    physics.player.air_lateral_deceleration = 0.0;
    physics
}

#[test]
fn a_floor_exit_leaves_at_the_angle_the_flight_went_in_at() {
    let physics = ballistic_physics();
    let jump = request([0.0, 8.0, 0.0], true, &[0.0, 8.0]);
    let landing = crossing(&preview(&physics, &jump)[0], 0, Phase::BeforeEntry)
        .point
        .map(|value| value as f32);
    let exit = floor_portal(20.0, 0.0, 20.0);
    // Dead centre, and drawn in from the side: both keep the 9 m/s the flight came in with.
    for (aside, entry) in [(0.0, Entry::Direct), (1.0, Entry::Funnel)] {
        let pair = through(&jump, floor_portal(landing[0] + aside, 0.0, landing[1]), Some(exit));
        let flown = &preview(&physics, &pair)[0];
        assert_eq!(flown.entry, Some(entry));
        let hop = flown.hop.expect("the flight enters");
        let (out, next) = (flown.path[hop + 1], flown.path[hop + 2]);
        let drift = (next[0] - out[0]).hypot(next[2] - out[2]) * 30.0;
        assert!((drift - 9.0).abs() < 0.05, "{entry:?}: {drift}");
        assert!(next[1] > out[1], "{entry:?}: {out:?} -> {next:?}");
        // It rises, drifts, and comes down well clear of the portal it left.
        let landed = crossing(flown, 0, Phase::AfterExit).point;
        assert!(
            (landed[0] - 20.0).hypot(landed[1] - 20.0) > 5.0,
            "{entry:?}: {landed:?}"
        );
        assert_eq!(flown.end, End::Below);
        assert!(flown.exit_range.is_empty());
    }
}

#[test]
fn a_flight_that_falls_back_into_a_portal_shows_where_steering_lands() {
    // Braked to a stop in the air, the body drops straight in and comes straight back up.
    let mut physics = game_physics();
    physics.player.air_deceleration = 60.0;
    let jump = request([0.0, 8.0, 0.0], true, &[0.0, 8.0]);
    let landing = crossing(&preview(&physics, &jump)[0], 0, Phase::BeforeEntry)
        .point
        .map(|value| value as f32);
    let pair = through(
        &jump,
        floor_portal(landing[0], 0.0, landing[1]),
        Some(floor_portal(20.0, 0.0, 20.0)),
    );
    let flown = &preview(&physics, &pair)[0];
    assert_eq!(flown.end, End::Reentered);
    // It passes the upper level's height twice over the portal, where no floor could catch it.
    let top = flown.path.iter().map(|point| point[1]).fold(f64::MIN, f64::max);
    assert!(top > 9.0, "{top}");
    assert!(
        flown
            .crossings
            .iter()
            .all(|crossing| crossing.phase == Phase::BeforeEntry),
        "{:?}",
        flown.crossings
    );
    // Steering is what lands it, so its range comes without being asked for:
    // beside the portal on its own level, and up on the level it rose past.
    for level in [0, 1] {
        let range = &flown
            .exit_range
            .iter()
            .find(|range| range.level == level)
            .unwrap_or_else(|| panic!("no steering range on level {level}"))
            .polygon;
        let (low, high) = bounds(range);
        assert!(range.len() >= 6 && high[0] - low[0] > 2.0, "level {level}: {range:?}");
    }
}

#[test]
fn meeting_portal_2_first_is_reversed() {
    let physics = game_physics();
    let jump = request([0.0, 8.0, 0.0], true, &[0.0, 8.0]);
    let landing = crossing(&preview(&physics, &jump)[0], 0, Phase::BeforeEntry)
        .point
        .map(|value| value as f32);
    let far = floor_portal(40.0, 0.0, 40.0);
    let flown = &preview(
        &physics,
        &through(&jump, far, Some(floor_portal(landing[0], 0.0, landing[1]))),
    )[0];
    assert_eq!(flown.entry, Some(Entry::Reversed));
    let out = flown.path[flown.hop.expect("the flight hops") + 1];
    assert!((out[0] - 40.0).abs() < 1.0 && (out[2] - 40.0).abs() < 1.5, "{out:?}");
}

#[test]
fn requests_are_validated() {
    let physics = physics_with(0.0);
    let mut still = request([0.0; 3], true, &[0.0]);
    still.takeoff.direction = [0.0, 0.0];
    assert!(jump_preview(&physics, &still).is_err());
    let mut late = request([0.0; 3], true, &[0.0]);
    late.takeoff.margin = -0.1;
    assert!(jump_preview(&physics, &late).is_err());
    let tall = request([0.0; 3], true, &[0.0; PREVIEW_MAX_HEIGHTS + 1]);
    assert!(jump_preview(&physics, &tall).is_err());
    let flat = through(&request([0.0; 3], true, &[0.0]), wall_portal([0.0; 3], [0.0; 3]), None);
    assert!(jump_preview(&physics, &flat).is_err());
}
