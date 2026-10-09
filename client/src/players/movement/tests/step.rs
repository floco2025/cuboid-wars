use super::super::fixtures::*;
use super::PlayerStepResult;
use crate::{
    portals::{PlayerHop, PlayerHopBody, player_hop},
    test_fixtures::portal,
};
use common::{
    config::GameplayConfig,
    physics::{
        PlayerFlightPortals, PlayerFlightState, character_movement_center, player_move_speed, step_player_flight,
    },
    protocol::{Portal, PortalEnd, RampDirection, RampShape, SwitchState, Wall},
};
use std::f32::consts::FRAC_PI_4;

const DELTA: f32 = 1.0 / 30.0;

// A world, its carriers and portals, and the settings the player's step runs with.
struct Scene {
    world: CollisionWorld,
    carriers: Carriers,
    portals: PortalSet,
    settings: MapSettings,
    gameplay: GameplayConfig,
}

impl Scene {
    fn new(layout: &MapLayout) -> Self {
        Self {
            world: CollisionWorld::from_map_layout(layout),
            carriers: Carriers::from_layout(layout),
            portals: PortalSet::default(),
            settings: map_settings(),
            gameplay: gameplay_config(),
        }
    }

    // `layout` as tick `tick` finds it.
    fn at_tick(layout: &MapLayout, tick: u32) -> Self {
        let (world, carriers) = world_at(layout, tick);
        Self {
            world,
            carriers,
            portals: PortalSet::default(),
            settings: map_settings(),
            gameplay: gameplay_config(),
        }
    }

    fn with_portals(mut self, portals: &[Portal]) -> Self {
        self.portals = PortalSet::rebuild(
            portals,
            &self.world,
            &self.carriers,
            self.gameplay.portals.size,
            player_physics(),
        );
        self
    }

    // A standing body at `start` with nothing held, for one tick.
    fn request(&self, start: Position) -> PlayerMovementStep<'_> {
        PlayerMovementStep {
            start,
            vertical_velocity: 0.0,
            horizontal_velocity: Vec3::ZERO,
            stance: PlayerStance::default(),
            intent: PlayerMoveIntent::NONE,
            has_speed: false,
            disabled: false,
            delta: DELTA,
            has_low_gravity: false,
            held_keys: &[],
            open_fields: &[],
            knockback_displacement: Vec3::ZERO,
            collision_world: &self.world,
            map_settings: &self.settings,
            gameplay_config: &self.gameplay,
            portal_set: &self.portals,
            carriers: &self.carriers,
        }
    }

    fn step(&self, pos: Position, velocity: Vec3, intent: PlayerMoveIntent, delta: f32) -> PlayerStepResult {
        step_player_movement(PlayerMovementStep {
            vertical_velocity: velocity.y,
            horizontal_velocity: velocity.with_y(0.0),
            intent,
            delta,
            ..self.request(pos)
        })
    }

    // The crossing `step` made, for a body without knockback.
    fn hop(&self, step: &PlayerStepResult, delta: f32) -> Option<PlayerHop> {
        player_hop(
            &self.portals,
            step.start.into(),
            step.movement.position.into(),
            &self.gameplay,
            &self.settings.movement,
            PlayerHopBody {
                stance: step.stance,
                knockback: &KnockbackVelocity::default(),
                horizontal_velocity: &HorizontalVelocity(step.horizontal_velocity),
                vertical_velocity: step.movement.vertical_velocity,
                carried: step.movement.carried(),
                yaw: 0.0,
            },
            delta,
        )
    }

    // Falls from `start` at `hz`, steering sideways from the `steer_after`th
    // crossing on, while the carriers run: the crossings, and where the body
    // ends up at what velocity.
    fn fall(
        &mut self,
        start: Vec3,
        velocity: Vec3,
        hz: usize,
        ticks: usize,
        steer_after: usize,
    ) -> (usize, Position, Vec3) {
        let delta = 1.0 / hz as f32;
        let mut pos = start.into();
        let mut velocity = velocity;
        let mut hops = 0;
        for tick in 0..ticks {
            self.carriers.advance(tick as u32 + 1, &SwitchState::default());
            self.world.set_carrier_poses(&self.carriers);
            self.portals.refresh(&self.carriers);
            let intent = if hops >= steer_after {
                PlayerMoveIntent {
                    sideways: 1.0,
                    ..PlayerMoveIntent::NONE
                }
            } else {
                PlayerMoveIntent::NONE
            };
            let step = self.step(pos, velocity, intent, delta);
            pos = step.movement.position;
            velocity = step.horizontal_velocity.with_y(step.movement.vertical_velocity);
            if let Some(hop) = self.hop(&step, delta) {
                pos = hop.crossing.origin.into();
                velocity = hop.crossing.horizontal_velocity.with_y(hop.crossing.vertical_velocity);
                hops += 1;
            }
        }
        (hops, pos, velocity)
    }
}

fn simulate(
    layout: MapLayout,
    start: Position,
    velocity: Vec3,
    intent: PlayerMoveIntent,
    stance: PlayerStance,
    ticks: usize,
) -> (Position, Vec3, PlayerStance) {
    let scene = Scene::new(&layout);
    let (mut pos, mut velocity, mut stance) = (start, velocity, stance);
    for _ in 0..ticks {
        let step = step_player_movement(PlayerMovementStep {
            vertical_velocity: velocity.y,
            horizontal_velocity: velocity.with_y(0.0),
            stance,
            intent,
            delta: 1.0 / 60.0,
            ..scene.request(pos)
        });
        pos = step.movement.position;
        velocity = step.horizontal_velocity.with_y(step.movement.vertical_velocity);
        stance = step.stance;
    }
    (pos, velocity, stance)
}

#[test]
fn jump_matches_reference_arc_and_retains_takeoff_motion() {
    let start = Position { x: 0.0, y: 0.0, z: 0.0 };
    let velocity = Vec3::new(4.375, 5.809475, 0.0);
    let (pos, v, _) = simulate(
        MapLayout::default(),
        start,
        velocity,
        PlayerMoveIntent::NONE,
        PlayerStance::default(),
        23,
    );
    assert!((pos.y - 1.125).abs() < 0.01, "apex {pos:?}");
    assert!((pos.x - 4.375 * 23.0 / 60.0).abs() < 0.001);
    assert_eq!(v.x, velocity.x);
}

#[test]
fn a_released_jump_and_forward_held_jump_follow_the_same_arc() {
    let start = Position {
        x: 0.0,
        y: 10.0,
        z: 0.0,
    };
    let launch = Vec3::new(0.0, 5.809475, 4.375);
    let released = simulate(
        MapLayout::default(),
        start,
        launch,
        PlayerMoveIntent::NONE,
        PlayerStance::default(),
        45,
    );
    let held = simulate(
        MapLayout::default(),
        start,
        launch,
        PlayerMoveIntent::moving(0.0),
        PlayerStance::default(),
        45,
    );
    assert_eq!(released, held);
}

#[test]
fn a_stationary_jump_can_accelerate_to_movement_speed_and_coast_after_release() {
    let mut scene = Scene::new(&MapLayout {
        floors: vec![lower_floor()],
        ..Default::default()
    });
    scene.settings.movement.gravity = 25.0;
    scene.settings.movement.player.jump_speed = 12.0;
    scene.settings.movement.player.air_acceleration = 45.0;
    for speed in [6.0, 9.0] {
        scene.settings.movement.player.move_speed = speed;
        for boosted in [false, true] {
            for yaw in [0.0, FRAC_PI_4] {
                let player = &scene.settings.movement.player;
                let target_speed = speed * if boosted { player.move_speed_power_up } else { 1.0 };
                let direction = Vec3::new(yaw.sin(), 0.0, yaw.cos());
                let mut pos = Position::default();
                let mut velocity = Vec3::Y * player.jump_speed;
                for tick in 0..28 {
                    // Jump vertically first, hold W while airborne, then release before landing.
                    let intent = if (6..26).contains(&tick) {
                        PlayerMoveIntent::moving(yaw)
                    } else {
                        PlayerMoveIntent::NONE
                    };
                    let before = pos;
                    let step = step_player_movement(PlayerMovementStep {
                        vertical_velocity: velocity.y,
                        horizontal_velocity: velocity.with_y(0.0),
                        intent,
                        has_speed: boosted,
                        ..scene.request(pos)
                    });
                    pos = step.movement.position;
                    velocity = step.horizontal_velocity.with_y(step.movement.vertical_velocity);
                    assert_eq!(step.movement.support, CharacterSupport::Airborne);
                    if tick < 6 {
                        assert_eq!(velocity.with_y(0.0), Vec3::ZERO);
                    } else if tick >= 26 {
                        let travelled = (Vec3::from(pos) - Vec3::from(before)).dot(direction);
                        assert!((travelled - target_speed / 30.0).abs() < 1e-4);
                    }
                }
                assert!((velocity.with_y(0.0) - direction * target_speed).length() < 1e-4);
                assert!(Vec3::from(pos).dot(direction) > target_speed * 0.3);
            }
        }
    }
}

#[test]
fn crouched_body_cannot_stand_inside_a_low_ceiling() {
    let mut ceiling = lower_floor();
    ceiling.y = 1.4;
    let layout = MapLayout {
        floors: vec![lower_floor(), ceiling],
        ..Default::default()
    };
    let start = Position::default();
    let crouched = PlayerStance { crouched: true };
    let (_, _, blocked) = simulate(layout, start, Vec3::ZERO, PlayerMoveIntent::NONE, crouched, 30);
    assert!(blocked.crouched);
    let (_, _, clear) = simulate(
        MapLayout {
            floors: vec![lower_floor()],
            ..Default::default()
        },
        start,
        Vec3::ZERO,
        PlayerMoveIntent::NONE,
        crouched,
        30,
    );
    assert!(!clear.crouched);
}

#[test]
fn airborne_duck_preserves_body_centre_and_launch_velocity() {
    let start = Position {
        x: 0.0,
        y: 10.0,
        z: 0.0,
    };
    let launch = Vec3::new(4.0, -2.0, 3.0);
    let standing = simulate(
        MapLayout::default(),
        start,
        launch,
        PlayerMoveIntent::NONE,
        PlayerStance::default(),
        1,
    );
    let ducked = simulate(
        MapLayout::default(),
        start,
        launch,
        PlayerMoveIntent {
            crouch: true,
            ..PlayerMoveIntent::NONE
        },
        PlayerStance::default(),
        1,
    );
    assert!((ducked.0.y - standing.0.y - 0.45).abs() < 1e-5);
    assert_eq!(ducked.1, standing.1);
    assert!(ducked.2.crouched);
    let unducked = simulate(
        MapLayout::default(),
        ducked.0,
        ducked.1,
        PlayerMoveIntent::NONE,
        ducked.2,
        1,
    );
    let continued = simulate(
        MapLayout::default(),
        standing.0,
        standing.1,
        PlayerMoveIntent::NONE,
        standing.2,
        1,
    );
    assert!((Vec3::from(unducked.0) - Vec3::from(continued.0)).length() < 1e-5);
}

#[test]
fn glancing_wall_contact_keeps_tangential_velocity_and_removes_inward_velocity() {
    let wall = Wall {
        z1: -100.0,
        z2: 100.0,
        height: 100.0,
        ..test_wall()
    };
    let layout = MapLayout {
        walls: vec![wall],
        ..Default::default()
    };
    let (pos, v, _) = simulate(
        layout,
        Position {
            x: -1.0,
            y: 10.0,
            z: 0.0,
        },
        Vec3::new(8.0, 0.0, 4.0),
        PlayerMoveIntent::NONE,
        PlayerStance::default(),
        15,
    );
    assert!(pos.x < -0.39, "wall penetration: {pos:?}");
    assert!(v.x.abs() < 0.01, "inward velocity {v:?}");
    assert!((v.z - 4.0).abs() < 0.01, "lost slide velocity {v:?}");
}

#[test]
fn equipment_changes_affect_acceleration_and_gravity_without_erasing_air_velocity() {
    let scene = Scene::new(&MapLayout::default());
    let request = PlayerMovementStep {
        vertical_velocity: -10.0,
        horizontal_velocity: Vec3::Z * 20.0,
        intent: PlayerMoveIntent::moving(0.0),
        has_speed: true,
        has_low_gravity: true,
        ..scene.request(Position {
            y: 20.0,
            ..Position::default()
        })
    };
    let boosted = step_player_movement(request);
    let erased = step_player_movement(PlayerMovementStep {
        has_speed: false,
        has_low_gravity: false,
        ..request
    });
    assert_eq!(boosted.horizontal_velocity, erased.horizontal_velocity);
    assert_eq!(erased.horizontal_velocity, Vec3::Z * 20.0);
    assert!((boosted.movement.vertical_velocity - (-10.0 - 5.0 / 30.0)).abs() < 1e-5);
    assert!((erased.movement.vertical_velocity - (-10.0 - 15.0 / 30.0)).abs() < 1e-5);
}

#[test]
fn passive_ground_and_air_braking_do_not_leave_reverse_velocity_after_a_blast() {
    let mut scene = Scene::new(&MapLayout {
        floors: vec![lower_floor()],
        ..Default::default()
    });
    scene.settings.movement.player.air_deceleration = 30.0;
    scene.settings.movement.player.air_lateral_deceleration = 40.0;
    for height in [0.0, 10.0] {
        let request = PlayerMovementStep {
            knockback_displacement: Vec3::X / 3.0,
            ..scene.request(Position {
                y: height,
                ..Position::default()
            })
        };
        let result = step_player_movement(request);
        assert!(result.movement.position.x > 0.3);
        assert_eq!(result.horizontal_velocity, Vec3::ZERO);
        let after = step_player_movement(PlayerMovementStep {
            start: result.movement.position,
            vertical_velocity: result.movement.vertical_velocity,
            knockback_displacement: Vec3::ZERO,
            ..request
        });
        assert_eq!(after.horizontal_velocity, Vec3::ZERO);
        assert_eq!(after.movement.position.x, result.movement.position.x);
    }
}

#[test]
fn a_rising_carrier_uses_ground_acceleration_and_inherits_velocity_once_on_jump() {
    let (mut carrier, floor) = slider();
    carrier.to = Position { x: 0.0, y: 4.0, z: 0.0 };
    let layout = MapLayout {
        carriers: vec![carrier],
        floors: vec![floor],
        ..Default::default()
    };
    let scene = Scene::at_tick(&layout, 10);
    let movement = &scene.settings.movement;
    let request = PlayerMovementStep {
        intent: PlayerMoveIntent::moving(0.0),
        ..scene.request(Position {
            x: 0.0,
            y: 4.0 * 9.0 / 60.0,
            z: 0.0,
        })
    };
    let grounded = step_player_movement(request);
    assert_eq!(grounded.movement.support, CharacterSupport::Ground);
    assert!((grounded.horizontal_velocity.z - 4.375 / 3.0).abs() < 1e-4);
    let jumped = step_player_movement(PlayerMovementStep {
        vertical_velocity: movement.player.jump_speed,
        ..request
    });
    assert!(
        (jumped.movement.vertical_velocity - (movement.player.jump_speed + 2.0 - movement.gravity / 30.0)).abs() < 1e-4
    );
    let next_scene = Scene::at_tick(&layout, 11);
    let next = step_player_movement(PlayerMovementStep {
        vertical_velocity: jumped.movement.vertical_velocity,
        horizontal_velocity: jumped.horizontal_velocity,
        intent: PlayerMoveIntent::moving(0.0),
        ..next_scene.request(jumped.movement.position)
    });
    assert!(
        (next.movement.vertical_velocity - (jumped.movement.vertical_velocity - movement.gravity / 30.0)).abs() < 1e-4
    );
}

// `test_ladder` between its ground and landing, with air rates too weak to
// carry a body over the crest by air control alone.
fn ladder_scene(ladder: Ladder) -> Scene {
    let mut scene = Scene::new(&MapLayout {
        floors: vec![ladder_front_base_floor(), ladder_back_landing_floor()],
        ladders: vec![ladder],
        ..Default::default()
    });
    let player = &mut scene.settings.movement.player;
    player.air_acceleration = 5.0;
    player.air_deceleration = 5.0;
    player.air_lateral_deceleration = 5.0;
    scene
}

// One intent held for a number of ticks; `jump` presses jump as the phase
// begins, from the support the previous tick left.
struct Phase {
    intent: PlayerMoveIntent,
    jump: bool,
    ticks: usize,
}

fn hold(intent: PlayerMoveIntent, ticks: usize) -> Phase {
    Phase {
        intent,
        jump: false,
        ticks,
    }
}

const TRACE_DELTA: f32 = 1.0 / 60.0;

fn trace(scene: &Scene, start: Position, phases: &[Phase]) -> Vec<(Position, CharacterSupport)> {
    let mut pos = start;
    let mut velocity = Vec3::ZERO;
    let mut stance = PlayerStance::default();
    let mut knockback = KnockbackVelocity::default();
    let mut support = CharacterSupport::Airborne;
    let mut out = Vec::new();
    for phase in phases {
        if phase.jump {
            let jump = player_jump(
                support,
                phase.intent,
                velocity.y,
                &scene.world,
                scene.gameplay.player.physics(),
                &scene.settings.movement,
                false,
                &pos,
                &[],
            );
            match jump {
                Some(PlayerJump::Rise(vertical)) => velocity.y = vertical,
                Some(PlayerJump::Release(shove)) => knockback.0 += shove,
                None => {}
            }
        }
        for _ in 0..phase.ticks {
            let step = step_player_movement(PlayerMovementStep {
                vertical_velocity: velocity.y,
                horizontal_velocity: velocity.with_y(0.0),
                stance,
                intent: phase.intent,
                delta: TRACE_DELTA,
                knockback_displacement: knockback.step(TRACE_DELTA),
                ..scene.request(pos)
            });
            pos = step.movement.position;
            velocity = step.horizontal_velocity.with_y(step.movement.vertical_velocity);
            stance = step.stance;
            support = step.movement.support;
            knockback.decay(TRACE_DELTA, scene.settings.movement.knockback.deceleration);
            out.push((pos, support));
        }
    }
    out
}

// Facing +Z into `test_ladder` from its front side, looking up.
const CLIMB: PlayerMoveIntent = PlayerMoveIntent {
    forward: 1.0,
    pitch: 1.0,
    ..PlayerMoveIntent::NONE
};
const LADDER_FOOT: Position = Position {
    x: 0.0,
    y: 0.0,
    z: -0.6,
};

#[test]
fn a_climber_steps_onto_the_landing_at_the_crest() {
    let scene = ladder_scene(test_ladder());
    let trace = trace(&scene, LADDER_FOOT, &[hold(CLIMB, 300)]);
    assert!(trace.iter().any(|(_, support)| *support == CharacterSupport::Ladder));
    assert!(
        trace.iter().any(|(pos, support)| {
            *support == CharacterSupport::Ground && pos.y > LEVEL_HEIGHT - 0.1 && pos.z > 0.3
        }),
        "never reached the landing: {:?}",
        trace.last()
    );
}

#[test]
fn a_climber_dismounts_sideways_at_ladder_speed() {
    let scene = ladder_scene(test_ladder());
    let sideways = PlayerMoveIntent {
        sideways: 1.0,
        ..PlayerMoveIntent::NONE
    };
    let trace = trace(&scene, LADDER_FOOT, &[hold(CLIMB, 20), hold(sideways, 30)]);
    assert_eq!(trace[19].1, CharacterSupport::Ladder);
    let (pos, support) = trace.last().expect("trace is empty");
    assert!(
        pos.x.abs() > 0.6 && *support != CharacterSupport::Ladder,
        "{pos:?} {support:?}"
    );
}

#[test]
fn a_jump_on_a_ladder_lets_go_without_rising() {
    let scene = ladder_scene(test_ladder());
    let let_go = Phase {
        intent: PlayerMoveIntent::NONE,
        jump: true,
        ticks: 45,
    };
    let trace = trace(&scene, LADDER_FOOT, &[hold(CLIMB, 20), let_go]);
    assert_eq!(trace[19].1, CharacterSupport::Ladder);
    // Letting go keeps only the climb velocity the body already carried.
    let movement = &scene.settings.movement;
    let ladder_speed = movement.player.move_speed * movement.player.move_speed_ladder;
    let coasting = ladder_speed * ladder_speed / (2.0 * movement.gravity);
    let release_y = trace[19].0.y;
    let highest = trace[20..].iter().map(|(pos, _)| pos.y).fold(f32::MIN, f32::max);
    assert!(
        highest <= release_y + coasting + 1e-3,
        "rose from {release_y} to {highest}"
    );
    let (pos, support) = trace.last().expect("trace is empty");
    assert!(
        scene.world.ladder_volume_at(pos).is_none() && *support != CharacterSupport::Ladder,
        "{pos:?} {support:?}"
    );
}

#[test]
fn a_forward_jump_lands_on_the_landing_behind_the_ladder() {
    let scene = ladder_scene(test_ladder_two_storey());
    let step_through = Phase {
        intent: PlayerMoveIntent::moving(0.0),
        jump: true,
        ticks: 60,
    };
    let trace = trace(&scene, LADDER_FOOT, &[hold(CLIMB, 120), step_through]);
    let (release, support) = trace[119];
    assert!(
        support == CharacterSupport::Ladder && release.y > LEVEL_HEIGHT,
        "{release:?} {support:?}"
    );
    assert!(
        trace[120..].iter().any(|(pos, support)| {
            *support == CharacterSupport::Ground && pos.z > 0.0 && (pos.y - LEVEL_HEIGHT).abs() < 0.05
        }),
        "never landed behind the ladder: {:?}",
        trace.last()
    );
}

#[test]
fn a_jump_while_descending_still_lets_go() {
    let scene = ladder_scene(test_ladder());
    let descend = PlayerMoveIntent {
        forward: -1.0,
        ..PlayerMoveIntent::NONE
    };
    let let_go = Phase {
        intent: descend,
        jump: true,
        ticks: 45,
    };
    // The climb velocity coasts out before the descent starts.
    let phases = [
        hold(CLIMB, 40),
        hold(PlayerMoveIntent::NONE, 15),
        hold(descend, 5),
        let_go,
    ];
    let trace = trace(&scene, LADDER_FOOT, &phases);
    assert_eq!(trace[59].1, CharacterSupport::Ladder);
    let (pos, support) = trace.last().expect("trace is empty");
    assert!(
        scene.world.ladder_volume_at(pos).is_none() && *support != CharacterSupport::Ladder,
        "{pos:?} {support:?}"
    );
}

// The slider's floor portal on the tile, paired with one high above it.
fn slider_portals(tile_end: PortalEnd, far_end: PortalEnd) -> [Portal; 2] {
    [
        Portal {
            carrier: TILE,
            ..portal(tile_end, Vec3::ZERO, Vec3::Y)
        },
        portal(far_end, Vec3::new(0.0, 8.0, 20.0), Vec3::NEG_Y),
    ]
}

fn slider_layout() -> MapLayout {
    let (carrier, floor) = slider();
    MapLayout {
        carriers: vec![carrier],
        floors: vec![floor],
        ..Default::default()
    }
}

// Walking off a sliding tile into its own floor portal: the tile's motion
// goes with the body as it sinks, so it keeps pace with the aperture, and
// comes off again at the crossing, since it is motion relative to the
// entry, not to the static exit.
#[test]
fn a_walk_into_a_sliding_tiles_portal_sinks_with_the_tile_and_leaves_its_speed_behind() {
    let layout = slider_layout();
    // On the tile, a step short of the aperture, walking toward it.
    let mut pos = Position {
        x: 4.0 * 10.0 / 60.0,
        y: 0.0,
        z: 1.45,
    };
    let (mut vertical, mut horizontal, mut stance) = (0.0, Vec3::ZERO, PlayerStance::default());
    let mut sinking: Vec<f32> = Vec::new();
    for tick in 10..40u32 {
        let scene = Scene::at_tick(&layout, tick).with_portals(&slider_portals(PortalEnd::A, PortalEnd::B));
        let step = step_player_movement(PlayerMovementStep {
            vertical_velocity: vertical,
            horizontal_velocity: horizontal,
            stance,
            intent: PlayerMoveIntent {
                forward: -1.0,
                ..PlayerMoveIntent::NONE
            },
            ..scene.request(pos)
        });
        pos = step.movement.position;
        vertical = step.movement.vertical_velocity;
        horizontal = step.horizontal_velocity;
        stance = step.stance;
        if step.movement.support == CharacterSupport::Airborne {
            sinking.push(pos.x - scene.carriers.pose(TILE).translation.x);
            assert!((horizontal.x - 2.0).abs() < 1e-3, "tick {tick}: {horizontal}");
        }
        if let Some(hop) = scene.hop(&step, DELTA) {
            assert!(sinking.len() >= 2, "{sinking:?}");
            let drift = sinking
                .iter()
                .fold(0.0_f32, |worst, rel| worst.max((rel - sinking[0]).abs()));
            assert!(drift < 1e-3, "drifted across the aperture: {sinking:?}");
            let walk = scene.settings.movement.player.move_speed;
            assert!(
                (hop.crossing.horizontal_velocity.length() - walk).abs() < 1e-3,
                "{hop:?}"
            );
            return;
        }
    }
    panic!("never crossed the tile's aperture: {pos:?}");
}

// Released flight over the slider's floor portal, from `feet` at tick 10:
// per tick, the position relative to the tile, horizontal velocity, and
// vertical velocity after the step.
fn slider_portal_flight(feet: Position, vertical: f32, horizontal: Vec3, ticks: u32) -> Vec<(Vec3, Vec3, f32)> {
    let layout = slider_layout();
    let (mut pos, mut vertical, mut horizontal, mut stance) = (feet, vertical, horizontal, PlayerStance::default());
    let mut trace = Vec::new();
    for tick in 10..10 + ticks {
        let scene = Scene::at_tick(&layout, tick).with_portals(&slider_portals(PortalEnd::B, PortalEnd::A));
        let step = step_player_movement(PlayerMovementStep {
            vertical_velocity: vertical,
            horizontal_velocity: horizontal,
            stance,
            ..scene.request(pos)
        });
        pos = step.movement.position;
        vertical = step.movement.vertical_velocity;
        horizontal = step.horizontal_velocity;
        stance = step.stance;
        assert_eq!(
            step.movement.support,
            CharacterSupport::Airborne,
            "tick {tick}: {pos:?}"
        );
        trace.push((
            Vec3::from(pos) - scene.carriers.pose(TILE).translation,
            horizontal,
            vertical,
        ));
    }
    trace
}

// A body coming up out of the tile's floor portal already moves with the
// tile; passing the top of the slab must not carry it a second time.
#[test]
fn rising_out_of_a_sliding_tiles_portal_is_not_carried_again() {
    let feet = Position {
        x: 10.0 * 4.0 / 60.0,
        y: -0.85,
        z: 0.0,
    };
    let trace = slider_portal_flight(feet, 6.0, Vec3::X * 2.0, 16);
    let (first, ..) = trace[0];
    let top = trace.iter().map(|(rel, ..)| rel.y).fold(f32::MIN, f32::max);
    assert!(
        top > 0.3 && trace.last().is_some_and(|(rel, ..)| rel.y < top),
        "{trace:?}"
    );
    for (rel, horizontal, _) in &trace {
        assert!((horizontal - Vec3::X * 2.0).length() < 1e-3, "{trace:?}");
        assert!((rel.x - first.x).abs() < 1e-3 && rel.z.abs() < 1e-3, "{trace:?}");
    }
}

// A body dropping into the tile's floor portal from the air never stood on
// the tile and takes none of its motion.
#[test]
fn dropping_into_a_sliding_tiles_portal_takes_none_of_its_motion() {
    let feet = Position {
        x: 10.0 * 4.0 / 60.0,
        y: 0.25,
        z: 0.0,
    };
    let trace = slider_portal_flight(feet, -1.0, Vec3::ZERO, 8);
    assert!(trace.last().is_some_and(|(rel, ..)| rel.y < -0.1), "{trace:?}");
    for (_, horizontal, _) in &trace {
        assert!(horizontal.length() < 1e-3, "{trace:?}");
    }
}

// The editor preview's `step_player_flight` held to this step.

const PICKUPS: [(bool, bool); 4] = [(false, false), (true, false), (false, true), (true, true)];

#[derive(Clone, Copy)]
enum Steering {
    Released,
    Heading(f32),
    Toward(Vec3),
}

impl Steering {
    fn intent(self, position: Vec3) -> PlayerMoveIntent {
        match self {
            Self::Released => PlayerMoveIntent::NONE,
            Self::Heading(yaw) => PlayerMoveIntent::moving(yaw),
            Self::Toward(target) => PlayerMoveIntent::moving((target.x - position.x).atan2(target.z - position.z)),
        }
    }
}

fn settings_with(air_rates: f32) -> MapSettings {
    let mut settings = map_settings();
    settings.movement.player = test_movement().player;
    settings.movement.player.air_acceleration = air_rates;
    settings.movement.player.air_deceleration = air_rates;
    settings.movement.player.air_lateral_deceleration = air_rates;
    settings
}

fn takeoff(settings: &MapSettings, has_speed: bool) -> PlayerFlightState {
    let speed = player_move_speed(&settings.movement.player, has_speed);
    PlayerFlightState {
        position: Vec3::new(0.0, 10.0, 0.0),
        horizontal_velocity: Vec3::new(0.6, 0.0, 0.8) * speed,
        vertical_velocity: settings.movement.player.jump_speed,
    }
}

#[test]
fn open_air_flight_matches_the_player_step_tick_for_tick() {
    let mut scene = Scene::new(&MapLayout::default());
    for air_rates in [5.0, 0.0] {
        scene.settings = settings_with(air_rates);
        for steering in [
            Steering::Released,
            Steering::Heading(2.0),
            Steering::Toward(Vec3::new(-6.0, 0.0, 3.0)),
        ] {
            for (has_speed, has_low_gravity) in PICKUPS {
                let player = &scene.settings.movement.player;
                let gravity = scene.settings.gravity_for(has_low_gravity);
                let mut flight = takeoff(&scene.settings, has_speed);
                let mut pos = Position::from(flight.position);
                let mut horizontal = flight.horizontal_velocity;
                let mut vertical = flight.vertical_velocity;
                for tick in 0..90 {
                    let intent = steering.intent(flight.position);
                    let step = step_player_movement(PlayerMovementStep {
                        vertical_velocity: vertical,
                        horizontal_velocity: horizontal,
                        intent,
                        has_speed,
                        has_low_gravity,
                        ..scene.request(pos)
                    });
                    pos = step.movement.position;
                    horizontal = step.horizontal_velocity;
                    vertical = step.movement.vertical_velocity;
                    let wish = intent.wish_velocity(player_move_speed(player, has_speed), true);
                    flight = step_player_flight(flight, wish, player, gravity, DELTA, None).state;
                    let context =
                        format!("air {air_rates}, speed {has_speed}, low gravity {has_low_gravity}, tick {tick}");
                    assert!(
                        Vec3::from(pos).distance(flight.position) < 1e-4,
                        "{context}: {pos:?} vs {:?}",
                        flight.position
                    );
                    assert!(
                        horizontal.distance(flight.horizontal_velocity) < 1e-4,
                        "{context}: {horizontal:?} vs {:?}",
                        flight.horizontal_velocity
                    );
                    assert!(
                        (vertical - flight.vertical_velocity).abs() < 1e-4,
                        "{context}: {vertical} vs {}",
                        flight.vertical_velocity
                    );
                }
            }
        }
    }
}

// Over every pickup combination and air rate.
struct Flown {
    fewest_hops: usize,
    most_hops: usize,
    always_funnelled: bool,
    ever_funnelled: bool,
}

fn gates(entry: (Vec3, Vec3), exit: (Vec3, Vec3)) -> Scene {
    Scene::new(&MapLayout::default()).with_portals(&[
        portal(PortalEnd::A, entry.0, entry.1),
        portal(PortalEnd::B, exit.0, exit.1),
    ])
}

// Flies the owner's step with its hop and the flight step side by side over
// every pickup combination and air rate, comparing body centres because a
// tilting hop crouches the owner about its centre.
fn assert_flights_match(scene: &mut Scene, start: PlayerFlightState, heading: Option<f32>, ticks: usize) -> Flown {
    let mut flown = Flown {
        fewest_hops: usize::MAX,
        most_hops: 0,
        always_funnelled: true,
        ever_funnelled: false,
    };
    for air_rates in [5.0, 0.0] {
        scene.settings = settings_with(air_rates);
        for (has_speed, has_low_gravity) in PICKUPS {
            let player = &scene.settings.movement.player;
            let standing = PlayerStance::default().physics(&scene.gameplay.player);
            let flight_portals = PlayerFlightPortals {
                set: &scene.portals,
                world: &scene.world,
                funnel: scene.gameplay.portals.funnel,
                body: standing,
            };
            let intent = heading.map_or(PlayerMoveIntent::NONE, PlayerMoveIntent::moving);
            let wish = intent.wish_velocity(player_move_speed(player, has_speed), true);
            let mut flight = start;
            let mut pos = Position::from(start.position);
            let mut horizontal = start.horizontal_velocity;
            let mut vertical = start.vertical_velocity;
            let mut stance = PlayerStance::default();
            let mut hops = 0;
            let mut funnelled = false;
            for tick in 0..ticks {
                let step = step_player_movement(PlayerMovementStep {
                    vertical_velocity: vertical,
                    horizontal_velocity: horizontal,
                    stance,
                    intent,
                    has_speed,
                    has_low_gravity,
                    ..scene.request(pos)
                });
                pos = step.movement.position;
                horizontal = step.horizontal_velocity;
                vertical = step.movement.vertical_velocity;
                stance = step.stance;
                let hop = scene.hop(&step, DELTA);
                if let Some(hop) = &hop {
                    pos = hop.crossing.origin.into();
                    horizontal = hop.crossing.horizontal_velocity;
                    vertical = hop.crossing.vertical_velocity;
                    if hop.force_crouch {
                        stance = PlayerStance { crouched: true };
                    }
                }
                let flown_tick = step_player_flight(
                    flight,
                    wish,
                    player,
                    scene.settings.gravity_for(has_low_gravity),
                    DELTA,
                    Some(&flight_portals),
                );
                flight = flown_tick.state;
                hops += usize::from(flown_tick.hop.is_some());
                funnelled |= flown_tick.funnelled;
                let context = format!("air {air_rates}, speed {has_speed}, low gravity {has_low_gravity}, tick {tick}");
                assert_eq!(hop.is_some(), flown_tick.hop.is_some(), "{context}");
                let center = character_movement_center(pos, stance.physics(&scene.gameplay.player));
                let flight_center = character_movement_center(flight.position.into(), standing);
                assert!(
                    center.distance(flight_center) < 1e-3,
                    "{context}: {center:?} vs {flight_center:?}"
                );
                assert!(
                    horizontal.distance(flight.horizontal_velocity) < 1e-3,
                    "{context}: {horizontal:?} vs {:?}",
                    flight.horizontal_velocity
                );
                assert!(
                    (vertical - flight.vertical_velocity).abs() < 1e-3,
                    "{context}: {vertical} vs {}",
                    flight.vertical_velocity
                );
            }
            flown.fewest_hops = flown.fewest_hops.min(hops);
            flown.most_hops = flown.most_hops.max(hops);
            flown.always_funnelled &= funnelled;
            flown.ever_funnelled |= funnelled;
        }
    }
    flown
}

fn falling(position: Vec3, drift: Vec3) -> PlayerFlightState {
    PlayerFlightState {
        position,
        horizontal_velocity: drift,
        vertical_velocity: -2.0,
    }
}

const FLOOR_GATE: (Vec3, Vec3) = (Vec3::ZERO, Vec3::Y);
const WALL_GATE: (Vec3, Vec3) = (Vec3::new(1.5, 1.3, 0.0), Vec3::NEG_X);
const FAR_WALL: (Vec3, Vec3) = (Vec3::new(30.0, 11.3, 5.0), Vec3::Z);
const FAR_FLOOR: (Vec3, Vec3) = (Vec3::new(30.0, 0.0, 5.0), Vec3::Y);

#[test]
fn a_fall_inside_the_margin_is_funnelled_through_like_the_player_step() {
    let flown = assert_flights_match(
        &mut gates(FLOOR_GATE, FAR_WALL),
        falling(Vec3::new(0.9, 6.0, 0.4), Vec3::new(-0.3, 0.0, 0.2)),
        None,
        90,
    );
    assert!(flown.always_funnelled && flown.fewest_hops == 1 && flown.most_hops == 1);
}

#[test]
fn a_fall_outside_the_margin_passes_the_gate_like_the_player_step() {
    let flown = assert_flights_match(
        &mut gates(FLOOR_GATE, FAR_WALL),
        falling(Vec3::new(3.0, 6.0, 3.0), Vec3::ZERO),
        None,
        90,
    );
    assert!(!flown.ever_funnelled && flown.most_hops == 0);
}

#[test]
fn held_input_over_a_floor_gate_is_never_funnelled() {
    let flown = assert_flights_match(
        &mut gates(FLOOR_GATE, FAR_WALL),
        falling(Vec3::new(0.9, 6.0, 0.4), Vec3::ZERO),
        Some(0.3),
        90,
    );
    assert!(!flown.ever_funnelled);
}

#[test]
fn a_walk_into_a_wall_gate_leaves_the_far_wall_like_the_player_step() {
    let walk = PlayerFlightState {
        position: Vec3::new(0.0, 0.3, 0.2),
        horizontal_velocity: Vec3::X * 6.0,
        vertical_velocity: 2.0,
    };
    let flown = assert_flights_match(&mut gates(WALL_GATE, FAR_WALL), walk, None, 90);
    assert!(!flown.ever_funnelled && flown.fewest_hops == 1 && flown.most_hops == 1);
    let flown = assert_flights_match(&mut gates(WALL_GATE, FAR_FLOOR), walk, None, 120);
    assert!(flown.fewest_hops >= 1);
}

#[test]
fn a_floor_to_floor_loop_keeps_matching_the_player_step() {
    let flown = assert_flights_match(
        &mut gates(FLOOR_GATE, (Vec3::new(10.0, 0.0, 0.0), Vec3::Y)),
        falling(Vec3::new(0.5, 6.0, 0.3), Vec3::ZERO),
        None,
        240,
    );
    assert!(flown.always_funnelled && flown.fewest_hops >= 2);
}

// A floor portal under a falling body, its exit facing down.
fn course(layout: MapLayout, exit: Vec3) -> Scene {
    Scene::new(&layout).with_portals(&[
        portal(PortalEnd::A, Vec3::ZERO, Vec3::Y),
        portal(PortalEnd::B, exit, Vec3::NEG_Y),
    ])
}

// A floor around the entry, the exit low above it so falls loop.
fn floor_course() -> Scene {
    course(
        MapLayout {
            floors: vec![Floor {
                x1: -10.0,
                x2: 10.0,
                z1: -10.0,
                z2: 10.0,
                y: 0.0,
                thickness: 0.4,
                level: 0,
                carrier: CarrierId::WORLD,
            }],
            ..Default::default()
        },
        Vec3::new(0.4, 4.0, 0.3),
    )
}

#[test]
fn edge_and_corner_falls_enter_across_gravities_and_tick_rates() {
    let mut course = floor_course();
    for gravity in [0.0, 5.0, 25.0] {
        course.settings.movement.gravity = gravity;
        for hz in [30, 60, 120] {
            for offset in [
                Vec3::new(1.25, 8.0, 0.0),
                Vec3::new(-1.25, 8.0, 1.85),
                Vec3::new(0.65, 8.0, -1.0),
            ] {
                for vy in [-3.0, -50.0] {
                    let (hops, pos, _) = course.fall(offset, Vec3::Y * vy, hz, hz * 4, usize::MAX);
                    assert!(
                        hops > 0,
                        "g={gravity}, hz={hz}, offset={offset:?}, vy={vy}, ended={pos:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn funnel_still_captures_with_air_braking_enabled_or_all_air_rates_zero() {
    for (acceleration, deceleration, lateral) in [(0.0, 0.0, 0.0), (20.0, 30.0, 40.0)] {
        let mut course = floor_course();
        let player = &mut course.settings.movement.player;
        player.air_acceleration = acceleration;
        player.air_deceleration = deceleration;
        player.air_lateral_deceleration = lateral;
        for hz in [30, 60, 120] {
            let (hops, pos, _) = course.fall(Vec3::new(1.2, 5.0, 0.5), Vec3::Y * -10.0, hz, hz * 2, usize::MAX);
            assert!(hops > 0, "{hz} Hz, air braking {deceleration}: {pos:?}");
        }
    }
}

#[test]
fn misaligned_loop_reaches_terminal_speed_and_steering_escapes() {
    let mut course = floor_course();
    course.settings.movement.gravity = 25.0;
    for hz in [30, 60, 120] {
        let (hops, pos, v) = course.fall(Vec3::new(0.7, 3.0, 0.5), Vec3::ZERO, hz, hz * 12, usize::MAX);
        assert!(hops >= 20 && v.y < -45.0, "hz={hz}, hops={hops}, pos={pos:?}, v={v:?}");
        let (hops, pos, _) = course.fall(Vec3::new(0.7, 3.0, 0.5), Vec3::ZERO, hz, hz * 5, 2);
        assert!(
            (2..10).contains(&hops) && pos.x > 3.0,
            "steering: hz={hz}, hops={hops}, pos={pos:?}"
        );
    }
}

#[test]
fn capture_obeys_margin_input_approach_and_disabled_setting() {
    let mut course = course(MapLayout::default(), Vec3::new(20.0, 4.0, 0.0));
    let pos = Position::from(Vec3::new(0.8, 3.0, 0.3));
    let falling = Vec3::Y * -5.0;
    let assisted = course.step(pos, falling, PlayerMoveIntent::NONE, 1.0 / 60.0);
    // The pull moves the body toward the centre and leaves its velocity alone.
    assert!(assisted.movement.position.x < pos.x);
    assert_eq!(assisted.horizontal_velocity, Vec3::ZERO);
    let funnel = course.gameplay.portals.funnel;
    for (start, v, intent) in [
        (Vec3::new(2.5, 3.0, 0.0).into(), falling, PlayerMoveIntent::NONE),
        (pos, -falling, PlayerMoveIntent::NONE),
        (
            pos,
            falling,
            PlayerMoveIntent {
                forward: 0.001,
                ..PlayerMoveIntent::NONE
            },
        ),
    ] {
        let enabled = course.step(start, v, intent, 1.0 / 60.0);
        course.gameplay.portals.funnel.capture_margin = 0.0;
        course.gameplay.portals.funnel.capture_growth = 0.0;
        let disabled = course.step(start, v, intent, 1.0 / 60.0);
        course.gameplay.portals.funnel = funnel;
        assert_eq!(enabled.movement.position, disabled.movement.position);
        assert_eq!(enabled.horizontal_velocity, disabled.horizontal_velocity);
    }
}

#[test]
fn a_floor_between_the_player_and_portal_prevents_capture() {
    let layout = MapLayout {
        floors: vec![Floor {
            x1: -5.0,
            x2: 5.0,
            z1: -5.0,
            z2: 5.0,
            y: 2.0,
            thickness: 0.4,
            level: 1,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let course = course(layout, Vec3::new(20.0, 4.0, 0.0));
    let step = course.step(
        Vec3::new(0.8, 4.0, 0.0).into(),
        Vec3::Y * -10.0,
        PlayerMoveIntent::NONE,
        1.0 / 60.0,
    );
    assert_eq!(step.horizontal_velocity, Vec3::ZERO);
    assert_eq!(step.movement.position.x, 0.8);
}

#[test]
fn capture_is_independent_of_movement_tuning_and_keeps_the_drift() {
    let mut course = course(MapLayout::default(), Vec3::new(20.0, 4.0, 0.0));
    let start = Vec3::new(0.9, 3.0, -0.5).into();
    let velocity = Vec3::new(-0.6, -10.0, 0.3);
    let a = course.step(start, velocity, PlayerMoveIntent::NONE, 1.0 / 60.0);
    course.settings.movement.player.move_speed = 30.0;
    course.settings.movement.player.air_acceleration = 0.1;
    let b = course.step(start, velocity, PlayerMoveIntent::NONE, 1.0 / 60.0);
    assert_eq!(a.movement.position, b.movement.position);
    assert_eq!(a.horizontal_velocity, b.horizontal_velocity);
    // With nothing braking it in the air, the sideways speed a body falls in
    // with is the sideways speed it leaves the other portal with.
    course.settings.movement.player.air_acceleration = 0.0;
    course.settings.movement.player.air_deceleration = 0.0;
    course.settings.movement.player.air_lateral_deceleration = 0.0;
    let (hops, pos, left) = course.fall(start.into(), velocity, 60, 60 * 4, usize::MAX);
    assert_eq!(hops, 1, "{pos:?}");
    assert!(
        (left.with_y(0.0).length() - velocity.with_y(0.0).length()).abs() < 1e-3,
        "{left:?}"
    );
}

#[test]
fn sloping_and_sliding_apertures_capture_off_center_falls() {
    let exit = portal(PortalEnd::B, Vec3::new(20.0, 4.0, 0.0), Vec3::NEG_Y);
    for hz in [30, 60, 120] {
        let mut ramp = Scene::new(&MapLayout {
            ramps: vec![Ramp {
                x1: -4.0,
                x2: 4.0,
                z1: -8.0,
                z2: 8.0,
                y: 0.0,
                height: 8.0,
                direction: RampDirection::South,
                shape: RampShape::Solid,
                thickness: 0.4,
                level: 0,
                levels: 2,
                carrier: CarrierId::WORLD,
            }],
            ..Default::default()
        })
        .with_portals(&[
            portal(
                PortalEnd::A,
                Vec3::new(0.0, 4.0, 0.0),
                Vec3::new(0.0, 1.0, -0.5).normalize(),
            ),
            exit,
        ]);
        let (hops, pos, _) = ramp.fall(Vec3::new(1.2, 10.0, 1.4), Vec3::Y * -20.0, hz, hz * 2, usize::MAX);
        assert!(hops > 0, "ramp {hz} Hz: {pos:?}");
        let mut sliding = Scene::new(&MapLayout {
            carriers: vec![Carrier {
                motion: Default::default(),
                initially_on: true,
                parent: CarrierId::WORLD,
                level: 0,
                levels: 0,
                from: Position::default(),
                to: (Vec3::X * 4.0).into(),
                travel_ticks: (hz * 4) as u32,
                pause_ticks: 0,
                phase_ticks: 0,
                switch: None,
            }],
            floors: vec![Floor {
                x1: -4.0,
                x2: 4.0,
                z1: -4.0,
                z2: 4.0,
                y: 0.0,
                thickness: 0.4,
                level: 0,
                carrier: CarrierId(1),
            }],
            ..Default::default()
        })
        .with_portals(&[
            Portal {
                carrier: CarrierId(1),
                ..portal(PortalEnd::A, Vec3::ZERO, Vec3::Y)
            },
            exit,
        ]);
        let (hops, pos, _) = sliding.fall(Vec3::new(1.2, 5.0, 0.5), Vec3::Y * -10.0, hz, hz * 2, usize::MAX);
        assert!(hops > 0, "moving {hz} Hz: {pos:?}");
    }
}

#[test]
fn an_apex_beside_a_ceiling_portal_is_not_an_approach() {
    let course = floor_course().with_portals(&[
        portal(PortalEnd::A, Vec3::new(20.0, 0.0, 0.0), Vec3::Y),
        portal(PortalEnd::B, Vec3::new(0.4, 4.0, 0.3), Vec3::NEG_Y),
    ]);
    let result = course.step(
        Vec3::new(0.8, 3.0, 0.3).into(),
        Vec3::ZERO,
        PlayerMoveIntent::NONE,
        1.0 / 60.0,
    );
    assert_eq!(result.horizontal_velocity, Vec3::ZERO);
    assert_eq!(result.movement.position.x, 0.8);
}
