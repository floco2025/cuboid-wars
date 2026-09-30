use super::{fixtures::*, player_step::map_settings};
use crate::{
    config::{GameplayConfig, gameplay::load_test_gameplay},
    constants::CHARACTER_TERMINAL_VELOCITY,
    physics::{
        HorizontalVelocity, KnockbackVelocity, PlayerFlightPortals, PlayerFlightState, PlayerHopBody,
        PlayerMovementStep, PortalSet, character_movement_center, player_move_speed, step_player_flight,
        step_player_movement,
    },
    protocol::{CarrierId, MapSettings, PlayerMoveIntent, PlayerStance, Portal, PortalEnd, PortalPairId, Position},
};

const DELTA: f32 = 1.0 / 30.0;

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

fn fly(
    settings: &MapSettings,
    steering: Steering,
    has_speed: bool,
    has_low_gravity: bool,
    ticks: usize,
) -> PlayerFlightState {
    let gravity = settings.gravity_for(has_low_gravity);
    let mut state = takeoff(settings, has_speed);
    for _ in 0..ticks {
        let wish = steering
            .intent(state.position)
            .wish_velocity(player_move_speed(&settings.movement.player, has_speed), true);
        state = step_player_flight(state, wish, &settings.movement.player, gravity, DELTA, None).state;
    }
    state
}

#[test]
fn open_air_flight_matches_the_player_step_tick_for_tick() {
    let world = CollisionWorld::from_map_layout(&MapLayout::default());
    let gameplay = load_test_gameplay().expect("gameplay");
    let carriers = Carriers::default();
    let portals = PortalSet::default();
    for air_rates in [5.0, 0.0] {
        let settings = settings_with(air_rates);
        for steering in [
            Steering::Released,
            Steering::Heading(2.0),
            Steering::Toward(Vec3::new(-6.0, 0.0, 3.0)),
        ] {
            for (has_speed, has_low_gravity) in [(false, false), (true, false), (false, true), (true, true)] {
                let mut flight = takeoff(&settings, has_speed);
                let mut pos = Position::from(flight.position);
                let mut horizontal = flight.horizontal_velocity;
                let mut vertical = flight.vertical_velocity;
                for tick in 0..90 {
                    let intent = steering.intent(flight.position);
                    let step = step_player_movement(PlayerMovementStep {
                        start: pos,
                        vertical_velocity: vertical,
                        horizontal_velocity: horizontal,
                        stance: PlayerStance::default(),
                        intent,
                        has_speed,
                        disabled: false,
                        delta: DELTA,
                        has_low_gravity,
                        held_keys: &[],
                        open_fields: &[],
                        external_displacement: Vec3::ZERO,
                        collision_world: &world,
                        map_settings: &settings,
                        gameplay_config: &gameplay,
                        portal_set: &portals,
                        carriers: &carriers,
                    });
                    pos = step.movement.position;
                    horizontal = step.horizontal_velocity;
                    vertical = step.movement.vertical_velocity;
                    let wish = intent.wish_velocity(player_move_speed(&settings.movement.player, has_speed), true);
                    flight = step_player_flight(
                        flight,
                        wish,
                        &settings.movement.player,
                        settings.gravity_for(has_low_gravity),
                        DELTA,
                        None,
                    )
                    .state;
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

#[test]
fn held_input_keeps_takeoff_speed_while_released_input_brakes() {
    let braking = settings_with(5.0);
    let speed = braking.movement.player.move_speed;
    let held = fly(&braking, Steering::Heading(0.6_f32.atan2(0.8)), false, false, 30);
    let released = fly(&braking, Steering::Released, false, false, 30);
    assert!((held.horizontal_velocity.length() - speed).abs() < 1e-4);
    assert!((released.horizontal_velocity.length() - (speed - 5.0)).abs() < 1e-3);

    let ballistic = settings_with(0.0);
    let held = fly(&ballistic, Steering::Heading(0.6_f32.atan2(0.8)), false, false, 30);
    let released = fly(&ballistic, Steering::Released, false, false, 30);
    assert!(held.horizontal_velocity.distance(released.horizontal_velocity) < 1e-6);
}

#[test]
fn terminal_velocity_caps_a_long_fall() {
    let fallen = fly(&settings_with(5.0), Steering::Released, false, false, 150);
    assert!((fallen.vertical_velocity + CHARACTER_TERMINAL_VELOCITY).abs() < 1e-6);
}

struct Gates {
    world: CollisionWorld,
    gameplay: GameplayConfig,
    carriers: Carriers,
    portals: PortalSet,
}

// Over every pickup combination and air rate.
struct Flown {
    fewest_hops: usize,
    most_hops: usize,
    always_funnelled: bool,
    ever_funnelled: bool,
}

fn gates(entry: (Vec3, Vec3), exit: (Vec3, Vec3)) -> Gates {
    let world = CollisionWorld::from_map_layout(&MapLayout::default());
    let gameplay = load_test_gameplay().expect("gameplay");
    let carriers = Carriers::default();
    let portal = |end, (center, normal): (Vec3, Vec3)| Portal {
        pair: PortalPairId(1),
        end,
        pos: center.into(),
        nx: normal.x,
        ny: normal.y,
        nz: normal.z,
        yaw: 0.0,
        carrier: CarrierId::WORLD,
    };
    let portals = PortalSet::rebuild(
        &[portal(PortalEnd::A, entry), portal(PortalEnd::B, exit)],
        &world,
        &carriers,
        gameplay.portals.size,
    );
    Gates {
        world,
        gameplay,
        carriers,
        portals,
    }
}

// Flies the owner's step with its hop and the flight step side by side over
// every pickup combination and air rate, comparing body centres because a
// tilting hop crouches the owner about its centre.
fn assert_flights_match(gates: &Gates, start: PlayerFlightState, heading: Option<f32>, ticks: usize) -> Flown {
    let mut flown = Flown {
        fewest_hops: usize::MAX,
        most_hops: 0,
        always_funnelled: true,
        ever_funnelled: false,
    };
    for air_rates in [5.0, 0.0] {
        let settings = settings_with(air_rates);
        for (has_speed, has_low_gravity) in [(false, false), (true, false), (false, true), (true, true)] {
            let standing = PlayerStance::default().physics(&gates.gameplay.player);
            let flight_portals = PlayerFlightPortals {
                set: &gates.portals,
                world: &gates.world,
                funnel: gates.gameplay.portals.funnel,
                body: standing,
            };
            let intent = heading.map_or(PlayerMoveIntent::NONE, PlayerMoveIntent::moving);
            let wish = intent.wish_velocity(player_move_speed(&settings.movement.player, has_speed), true);
            let mut flight = start;
            let mut pos = Position::from(start.position);
            let mut horizontal = start.horizontal_velocity;
            let mut vertical = start.vertical_velocity;
            let mut stance = PlayerStance::default();
            let mut hops = 0;
            let mut funnelled = false;
            for tick in 0..ticks {
                let step = step_player_movement(PlayerMovementStep {
                    start: pos,
                    vertical_velocity: vertical,
                    horizontal_velocity: horizontal,
                    stance,
                    intent,
                    has_speed,
                    disabled: false,
                    delta: DELTA,
                    has_low_gravity,
                    held_keys: &[],
                    open_fields: &[],
                    external_displacement: Vec3::ZERO,
                    collision_world: &gates.world,
                    map_settings: &settings,
                    gameplay_config: &gates.gameplay,
                    portal_set: &gates.portals,
                    carriers: &gates.carriers,
                });
                pos = step.movement.position;
                horizontal = step.horizontal_velocity;
                vertical = step.movement.vertical_velocity;
                stance = step.stance;
                let hop = gates.portals.player_hop(
                    step.start.into(),
                    pos.into(),
                    &gates.gameplay,
                    &settings.movement,
                    PlayerHopBody {
                        stance,
                        knockback: &KnockbackVelocity::default(),
                        horizontal_velocity: &HorizontalVelocity(horizontal),
                        vertical_velocity: vertical,
                        yaw: 0.0,
                    },
                );
                if let Some(hop) = &hop {
                    pos = hop.origin.into();
                    horizontal = hop.horizontal_velocity;
                    vertical = hop.vertical_velocity;
                    if hop.force_crouch {
                        stance = PlayerStance {
                            crouched: true,
                            fraction: 1.0,
                        };
                    }
                }
                let flown_tick = step_player_flight(
                    flight,
                    wish,
                    &settings.movement.player,
                    settings.gravity_for(has_low_gravity),
                    DELTA,
                    Some(&flight_portals),
                );
                flight = flown_tick.state;
                hops += usize::from(flown_tick.hop.is_some());
                funnelled |= flown_tick.funnelled;
                let context = format!("air {air_rates}, speed {has_speed}, low gravity {has_low_gravity}, tick {tick}");
                assert_eq!(hop.is_some(), flown_tick.hop.is_some(), "{context}");
                let center = character_movement_center(pos, stance.physics(&gates.gameplay.player));
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
        &gates(FLOOR_GATE, FAR_WALL),
        falling(Vec3::new(0.9, 6.0, 0.4), Vec3::new(-0.3, 0.0, 0.2)),
        None,
        90,
    );
    assert!(flown.always_funnelled && flown.fewest_hops == 1 && flown.most_hops == 1);
}

#[test]
fn a_fall_outside_the_margin_passes_the_gate_like_the_player_step() {
    let flown = assert_flights_match(
        &gates(FLOOR_GATE, FAR_WALL),
        falling(Vec3::new(3.0, 6.0, 3.0), Vec3::ZERO),
        None,
        90,
    );
    assert!(!flown.ever_funnelled && flown.most_hops == 0);
}

#[test]
fn held_input_over_a_floor_gate_is_never_funnelled() {
    let flown = assert_flights_match(
        &gates(FLOOR_GATE, FAR_WALL),
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
    let flown = assert_flights_match(&gates(WALL_GATE, FAR_WALL), walk, None, 90);
    assert!(!flown.ever_funnelled && flown.fewest_hops == 1 && flown.most_hops == 1);
    let flown = assert_flights_match(&gates(WALL_GATE, FAR_FLOOR), walk, None, 120);
    assert!(flown.fewest_hops >= 1);
}

#[test]
fn a_floor_to_floor_loop_keeps_matching_the_player_step() {
    let flown = assert_flights_match(
        &gates(FLOOR_GATE, (Vec3::new(10.0, 0.0, 0.0), Vec3::Y)),
        falling(Vec3::new(0.5, 6.0, 0.3), Vec3::ZERO),
        None,
        240,
    );
    assert!(flown.always_funnelled && flown.fewest_hops >= 2);
}
