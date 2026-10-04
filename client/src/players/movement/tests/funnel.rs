use super::super::fixtures::*;
use crate::portals::{PlayerHopBody, player_hop};
use common::protocol::{Portal, PortalEnd, PortalPairId, Ramp, RampDirection, RampShape, SwitchState};

fn portal(end: PortalEnd, center: Vec3, normal: Vec3) -> Portal {
    Portal {
        pair: PortalPairId(1),
        end,
        pos: center.into(),
        nx: normal.x,
        ny: normal.y,
        nz: normal.z,
        yaw: 0.0,
        carrier: CarrierId::WORLD,
    }
}

struct Course {
    world: CollisionWorld,
    config: GameplayConfig,
    settings: MapSettings,
    portals: PortalSet,
    carriers: Carriers,
}

impl Course {
    fn new() -> Self {
        let layout = MapLayout {
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
        };
        Self::from_layout(layout, Vec3::new(0.4, 4.0, 0.3))
    }
    fn from_layout(layout: MapLayout, exit: Vec3) -> Self {
        let world = CollisionWorld::from_map_layout(&layout);
        let config = gameplay_config();
        let carriers = Carriers::from_layout(&layout);
        let portals = PortalSet::rebuild(
            &[
                portal(PortalEnd::A, Vec3::ZERO, Vec3::Y),
                portal(PortalEnd::B, exit, Vec3::NEG_Y),
            ],
            &world,
            &carriers,
            config.portals.size, player_physics());
        Self {
            world,
            config,
            settings: map_settings(),
            portals,
            carriers,
        }
    }
    fn step(
        &self,
        pos: Position,
        velocity: Vec3,
        intent: PlayerMoveIntent,
        dt: f32,
    ) -> super::super::PlayerStepResult {
        step_player_movement(PlayerMovementStep {
            start: pos,
            vertical_velocity: velocity.y,
            horizontal_velocity: velocity.with_y(0.0),
            stance: PlayerStance::default(),
            intent,
            has_speed: false,
            disabled: false,
            delta: dt,
            has_low_gravity: false,
            held_keys: &[],
            open_fields: &[],
            knockback_displacement: Vec3::ZERO,
            collision_world: &self.world,
            map_settings: &self.settings,
            gameplay_config: &self.config,
            portal_set: &self.portals,
            carriers: &self.carriers,
        })
    }
    fn fall(
        &mut self,
        start: Vec3,
        velocity: Vec3,
        hz: usize,
        ticks: usize,
        steer_after: usize,
    ) -> (usize, Position, Vec3) {
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
            let step = self.step(pos, velocity, intent, 1.0 / hz as f32);
            let from = step.start;
            pos = step.movement.position;
            velocity = step.horizontal_velocity.with_y(step.movement.vertical_velocity);
            if let Some(hop) = player_hop(
                &self.portals,
                from.into(),
                pos.into(),
                &self.config,
                &self.settings.movement,
                PlayerHopBody {
                    stance: step.stance,
                    knockback: &KnockbackVelocity::default(),
                    horizontal_velocity: &HorizontalVelocity(velocity.with_y(0.0)),
                    vertical_velocity: velocity.y,
                    carried: step.movement.carried(),
                    yaw: 0.0,
                },
                1.0 / hz as f32,
            ) {
                pos = hop.crossing.origin.into();
                velocity = hop.crossing.horizontal_velocity.with_y(hop.crossing.vertical_velocity);
                hops += 1;
            }
        }
        (hops, pos, velocity)
    }
}

#[test]
fn edge_and_corner_falls_enter_across_gravities_and_tick_rates() {
    let mut course = Course::new();
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
        let mut course = Course::new();
        course.settings.movement.player.air_acceleration = acceleration;
        course.settings.movement.player.air_deceleration = deceleration;
        course.settings.movement.player.air_lateral_deceleration = lateral;
        for hz in [30, 60, 120] {
            let (hops, pos, _) = course.fall(Vec3::new(1.2, 5.0, 0.5), Vec3::Y * -10.0, hz, hz * 2, usize::MAX);
            assert!(hops > 0, "{hz} Hz, air braking {deceleration}: {pos:?}");
        }
    }
}

#[test]
fn misaligned_loop_reaches_terminal_speed_and_steering_escapes() {
    let mut course = Course::new();
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
    let mut course = Course::from_layout(MapLayout::default(), Vec3::new(20.0, 4.0, 0.0));
    let pos = Position::from(Vec3::new(0.8, 3.0, 0.3));
    let falling = Vec3::Y * -5.0;
    let assisted = course.step(pos, falling, PlayerMoveIntent::NONE, 1.0 / 60.0);
    // The pull moves the body toward the centre and leaves its velocity alone.
    assert!(assisted.movement.position.x < pos.x);
    assert_eq!(assisted.horizontal_velocity, Vec3::ZERO);
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
        course.config.portals.funnel.capture_margin = 0.0;
        course.config.portals.funnel.capture_growth = 0.0;
        let disabled = course.step(start, v, intent, 1.0 / 60.0);
        course.config.portals.funnel.capture_margin = 0.6;
        course.config.portals.funnel.capture_growth = 1.0;
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
    let course = Course::from_layout(layout, Vec3::new(20.0, 4.0, 0.0));
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
    let mut course = Course::from_layout(MapLayout::default(), Vec3::new(20.0, 4.0, 0.0));
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
    for hz in [30, 60, 120] {
        let mut ramp = Course::from_layout(
            MapLayout {
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
            },
            Vec3::new(20.0, 4.0, 0.0),
        );
        ramp.portals = PortalSet::rebuild(
            &[
                portal(
                    PortalEnd::A,
                    Vec3::new(0.0, 4.0, 0.0),
                    Vec3::new(0.0, 1.0, -0.5).normalize(),
                ),
                portal(PortalEnd::B, Vec3::new(20.0, 4.0, 0.0), Vec3::NEG_Y),
            ],
            &ramp.world,
            &ramp.carriers,
            ramp.config.portals.size, player_physics());
        let (hops, pos, _) = ramp.fall(Vec3::new(1.2, 10.0, 1.4), Vec3::Y * -20.0, hz, hz * 2, usize::MAX);
        assert!(hops > 0, "ramp {hz} Hz: {pos:?}");
        let mut sliding = Course::from_layout(
            MapLayout {
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
            },
            Vec3::new(20.0, 4.0, 0.0),
        );
        let mut entry = portal(PortalEnd::A, Vec3::ZERO, Vec3::Y);
        entry.carrier = CarrierId(1);
        sliding.portals = PortalSet::rebuild(
            &[entry, portal(PortalEnd::B, Vec3::new(20.0, 4.0, 0.0), Vec3::NEG_Y)],
            &sliding.world,
            &sliding.carriers,
            sliding.config.portals.size, player_physics());
        let (hops, pos, _) = sliding.fall(Vec3::new(1.2, 5.0, 0.5), Vec3::Y * -10.0, hz, hz * 2, usize::MAX);
        assert!(hops > 0, "moving {hz} Hz: {pos:?}");
    }
}

#[test]
fn an_apex_beside_a_ceiling_portal_is_not_an_approach() {
    let mut course = Course::new();
    course.portals = PortalSet::rebuild(
        &[
            portal(PortalEnd::A, Vec3::new(20.0, 0.0, 0.0), Vec3::Y),
            portal(PortalEnd::B, Vec3::new(0.4, 4.0, 0.3), Vec3::NEG_Y),
        ],
        &course.world,
        &course.carriers,
        course.config.portals.size, player_physics());
    let result = course.step(
        Vec3::new(0.8, 3.0, 0.3).into(),
        Vec3::ZERO,
        PlayerMoveIntent::NONE,
        1.0 / 60.0,
    );
    assert_eq!(result.horizontal_velocity, Vec3::ZERO);
    assert_eq!(result.movement.position.x, 0.8);
}
