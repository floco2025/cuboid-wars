use super::*;
use crate::constants::CHARACTER_TERMINAL_VELOCITY;

// Mirrors one server tick: the movement step (portal backing excluded,
// so the body sinks straight through), then the crossing check between
// the previous and current positions.
#[test]
fn perpetual_floor_fall_keeps_its_speed_across_hops() {
    let physics = player_physics();
    let layout = MapLayout {
        floors: vec![
            Floor {
                x1: -10.0,
                z1: -10.0,
                x2: 10.0,
                z2: 10.0,
                y: 0.0,
                thickness: FLOOR_THICKNESS,
                level: 0,
                carrier: CarrierId::WORLD,
            },
            Floor {
                x1: 40.0,
                z1: 40.0,
                x2: 60.0,
                z2: 60.0,
                y: 0.0,
                thickness: FLOOR_THICKNESS,
                level: 0,
                carrier: CarrierId::WORLD,
            },
        ],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout);
    let set = PortalSet::rebuild(
        &[
            portal(PortalEnd::A, Vec3::new(0.0, 0.0, 0.0), Vec3::Y, 0.0),
            portal(PortalEnd::B, Vec3::new(50.0, 0.0, 50.0), Vec3::Y, 0.0),
        ],
        &world,
        &Carriers::default(),
    );
    let env = CharacterEnvironment {
        ladder_mode: LadderMode::Automatic,
        collision_world: &world,
        gravity: 25.0,
        passable_kinds: &[],
        physics,
        ladder_climb_ratio: LADDER_CLIMB_RATIO,
        portals: Some(&set),
        carriers: &Carriers::default(),
    };

    let mut pos = Position { x: 0.0, y: 8.0, z: 0.0 };
    let mut vertical_velocity = 0.0_f32;
    let mut entry_speeds: Vec<f32> = Vec::new();

    for _ in 0..(30 * 8) {
        let from = pos;
        let result = step_character_movement(
            CharacterStep {
                start: pos,
                vertical_velocity,
                control_velocity: Vec3::ZERO,
                external_displacement: Vec3::ZERO,
                delta: TICK_SECS,
            },
            &env,
        );
        pos = result.position;
        vertical_velocity = result.vertical_velocity;
        if let Some(hop) = set.character_hop(
            Vec3::from(from),
            Vec3::from(pos),
            physics,
            CharacterHopBody {
                control_velocity: Vec3::ZERO,
                knockback: Vec3::ZERO,
                airborne_momentum: Vec3::ZERO,
                vertical_velocity,
                yaw: 0.0,
            },
            22.5,
        ) {
            entry_speeds.push(-vertical_velocity);
            pos = hop.origin.into();
            vertical_velocity = hop.vertical_velocity;
        }
    }

    assert!(
        entry_speeds.len() >= 3,
        "only {} hops in 8 s: {entry_speeds:?}",
        entry_speeds.len()
    );
    let first = entry_speeds[0];
    let last = *entry_speeds.last().expect("no hops recorded");
    let expected = (2.0 * env.gravity * 8.0).sqrt().min(CHARACTER_TERMINAL_VELOCITY);
    assert!(first > expected - 3.0, "first entry too slow: {entry_speeds:?}");
    assert!(last > first - 3.0, "speed decayed across hops: {entry_speeds:?}");
}

// The fast cycle: floor portal with its pair on the ceiling directly
// above. Every pass adds a room of gravity; speed must build to the
// terminal cap and stay there.
#[test]
fn floor_to_ceiling_fall_accelerates_toward_terminal_velocity() {
    let physics = player_physics();
    let layout = MapLayout {
        floors: vec![Floor {
            x1: -10.0,
            z1: -10.0,
            x2: 10.0,
            z2: 10.0,
            y: 0.0,
            thickness: FLOOR_THICKNESS,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout);
    let set = PortalSet::rebuild(
        &[
            portal(PortalEnd::A, Vec3::new(0.0, 0.0, 0.0), Vec3::Y, 0.0),
            portal(PortalEnd::B, Vec3::new(0.0, 4.0, 0.0), Vec3::NEG_Y, 0.0),
        ],
        &world,
        &Carriers::default(),
    );
    let env = CharacterEnvironment {
        ladder_mode: LadderMode::Automatic,
        collision_world: &world,
        gravity: 25.0,
        passable_kinds: &[],
        physics,
        ladder_climb_ratio: LADDER_CLIMB_RATIO,
        portals: Some(&set),
        carriers: &Carriers::default(),
    };

    let mut pos = Position { x: 0.0, y: 3.0, z: 0.0 };
    let mut vertical_velocity = 0.0_f32;
    let mut entry_speeds: Vec<f32> = Vec::new();

    for _ in 0..(30 * 8) {
        let from = pos;
        let result = step_character_movement(
            CharacterStep {
                start: pos,
                vertical_velocity,
                control_velocity: Vec3::ZERO,
                external_displacement: Vec3::ZERO,
                delta: TICK_SECS,
            },
            &env,
        );
        pos = result.position;
        vertical_velocity = result.vertical_velocity;
        if let Some(hop) = set.character_hop(
            Vec3::from(from),
            Vec3::from(pos),
            physics,
            CharacterHopBody {
                control_velocity: Vec3::ZERO,
                knockback: Vec3::ZERO,
                airborne_momentum: Vec3::ZERO,
                vertical_velocity,
                yaw: 0.0,
            },
            22.5,
        ) {
            entry_speeds.push(-vertical_velocity);
            pos = hop.origin.into();
            vertical_velocity = hop.vertical_velocity;
        }
    }

    assert!(
        entry_speeds.len() >= 8,
        "only {} hops in 8 s: {entry_speeds:?}",
        entry_speeds.len()
    );
    let last = *entry_speeds.last().expect("no hops recorded");
    assert!(
        last > CHARACTER_TERMINAL_VELOCITY - 2.0,
        "fall chain never reached terminal velocity: {entry_speeds:?}"
    );
    for window in entry_speeds.windows(2) {
        assert!(
            window[1] > window[0] - 0.5,
            "speed regressed mid-chain: {entry_speeds:?}"
        );
    }
}

#[test]
fn aperture_offset_carries_through_an_opposing_pair() {
    // Floor -> ceiling: the mapped offset preserves world drift, so a
    // steering player accumulates displacement across hops.
    let set = pair(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::Y,
        Vec3::new(10.0, 4.0, 10.0),
        Vec3::NEG_Y,
    );
    let hop = set
        .character_hop(
            Vec3::new(0.0, -0.85, 0.5),
            Vec3::new(0.0, -0.95, 0.5),
            player_physics(),
            CharacterHopBody {
                control_velocity: Vec3::ZERO,
                knockback: Vec3::ZERO,
                airborne_momentum: Vec3::ZERO,
                vertical_velocity: -5.0,
                yaw: 0.0,
            },
            CAP,
        )
        .expect("offset crossing did not trigger");
    assert!((hop.origin.x - 10.0).abs() < 1e-4);
    assert!((hop.origin.z - 10.5).abs() < 1e-4);
}

#[test]
fn carried_offset_is_clamped_to_the_exit_aperture() {
    let set = pair(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::Y,
        Vec3::new(10.0, 4.0, 10.0),
        Vec3::NEG_Y,
    );
    let physics = player_physics();
    let hop = set
        .character_hop(
            Vec3::new(0.55, -0.85, 0.0),
            Vec3::new(0.55, -0.95, 0.0),
            physics,
            CharacterHopBody {
                control_velocity: Vec3::ZERO,
                knockback: Vec3::ZERO,
                airborne_momentum: Vec3::ZERO,
                vertical_velocity: -5.0,
                yaw: 0.0,
            },
            CAP,
        )
        .expect("edge crossing did not trigger");
    let limit = PORTAL_HALF_WIDTH - physics.movement_collider.radius();
    assert!((hop.origin.x - 10.0).abs() <= limit + 1e-4);
    assert!(hop.origin.x > 10.0);
}

// Holding a direction while looping must break the loop within a few
// hops: the crossing gate is aperture-bound, so accumulated drift makes
// the body miss the hole and land beside it.
#[test]
fn steering_sideways_escapes_a_portal_fall_chain() {
    let physics = player_physics();
    let layout = MapLayout {
        floors: vec![Floor {
            x1: -10.0,
            z1: -10.0,
            x2: 10.0,
            z2: 10.0,
            y: 0.0,
            thickness: FLOOR_THICKNESS,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout);
    let set = PortalSet::rebuild(
        &[
            portal(PortalEnd::A, Vec3::new(0.0, 0.0, 0.0), Vec3::Y, 0.0),
            portal(PortalEnd::B, Vec3::new(0.0, 4.0, 0.0), Vec3::NEG_Y, 0.0),
        ],
        &world,
        &Carriers::default(),
    );
    let env = CharacterEnvironment {
        ladder_mode: LadderMode::Automatic,
        collision_world: &world,
        gravity: 25.0,
        passable_kinds: &[],
        physics,
        ladder_climb_ratio: LADDER_CLIMB_RATIO,
        portals: Some(&set),
        carriers: &Carriers::default(),
    };

    let mut pos = Position { x: 0.0, y: 3.0, z: 0.0 };
    let mut vertical_velocity = 0.0_f32;
    let mut hops = 0;

    for _ in 0..(30 * 8) {
        // Fall in hands-off, then steer once the chain is running.
        let control = if hops >= 1 {
            Vec3::new(0.0, 0.0, 6.0)
        } else {
            Vec3::ZERO
        };
        let from = pos;
        let result = step_character_movement(
            CharacterStep {
                start: pos,
                vertical_velocity,
                control_velocity: control,
                external_displacement: Vec3::ZERO,
                delta: TICK_SECS,
            },
            &env,
        );
        pos = result.position;
        vertical_velocity = result.vertical_velocity;
        if let Some(hop) = set.character_hop(
            Vec3::from(from),
            Vec3::from(pos),
            physics,
            CharacterHopBody {
                control_velocity: control,
                knockback: Vec3::ZERO,
                airborne_momentum: Vec3::ZERO,
                vertical_velocity,
                yaw: 0.0,
            },
            22.5,
        ) {
            pos = hop.origin.into();
            vertical_velocity = hop.vertical_velocity;
            hops += 1;
        }
    }

    assert!(hops >= 1, "the chain never started");
    assert!(hops <= 10, "steering never escaped the chain: {hops} hops");
    assert!(pos.z > 2.0, "escaped body did not keep moving: z = {}", pos.z);
}

#[test]
fn falling_toward_a_floor_portal_funnels_toward_its_axis() {
    let set = pair(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::Y,
        Vec3::new(10.0, 4.0, 10.0),
        Vec3::NEG_Y,
    );
    let pull = set.funnel_displacement(Vec3::new(0.5, 2.0, -0.3), player_physics(), Vec3::ZERO, -10.0, 0.1);
    assert!(pull.x < 0.0, "pull should point back toward the axis: {pull:?}");
    assert!(pull.z > 0.0, "pull should point back toward the axis: {pull:?}");
    assert!(pull.y == 0.0);
}

#[test]
fn floor_portal_funnel_is_symmetric_through_character_movement() {
    let physics = player_physics();
    let layout = MapLayout {
        floors: vec![Floor {
            x1: -10.0,
            z1: -10.0,
            x2: 10.0,
            z2: 10.0,
            y: 0.0,
            thickness: FLOOR_THICKNESS,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout);
    let set = PortalSet::rebuild(
        &[
            portal(PortalEnd::A, Vec3::ZERO, Vec3::Y, 0.0),
            portal(PortalEnd::B, Vec3::new(8.0, 4.0, 8.0), Vec3::NEG_Y, 0.0),
        ],
        &world,
        &Carriers::default(),
    );
    let env = CharacterEnvironment {
        ladder_mode: LadderMode::Automatic,
        collision_world: &world,
        gravity: 25.0,
        passable_kinds: &[],
        physics,
        ladder_climb_ratio: LADDER_CLIMB_RATIO,
        portals: Some(&set),
        carriers: &Carriers::default(),
    };

    let step_from = |x| {
        step_character_movement(
            CharacterStep {
                start: Position { x, y: 0.0, z: 0.0 },
                vertical_velocity: -10.0,
                control_velocity: Vec3::ZERO,
                external_displacement: Vec3::ZERO,
                delta: TICK_SECS,
            },
            &env,
        )
    };
    let from_left = step_from(-0.5);
    let from_right = step_from(0.5);

    assert!(from_left.position.x > -0.5, "left approach was repelled: {from_left:?}");
    assert!(
        from_right.position.x < 0.5,
        "right approach was repelled: {from_right:?}"
    );
    assert!((from_left.position.x + from_right.position.x).abs() < 1e-4);
}

#[test]
fn steering_disengages_the_funnel() {
    let set = pair(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::Y,
        Vec3::new(10.0, 4.0, 10.0),
        Vec3::NEG_Y,
    );
    let pull = set.funnel_displacement(
        Vec3::new(0.5, 2.0, 0.0),
        player_physics(),
        Vec3::new(6.0, 0.0, 0.0),
        -10.0,
        0.1,
    );
    assert_eq!(pull, Vec3::ZERO);
}

#[test]
fn rising_away_from_a_floor_portal_is_not_funneled() {
    let set = pair(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::Y,
        Vec3::new(10.0, 4.0, 10.0),
        Vec3::NEG_Y,
    );
    let pull = set.funnel_displacement(Vec3::new(0.5, 2.0, 0.0), player_physics(), Vec3::ZERO, 10.0, 0.1);
    assert_eq!(pull, Vec3::ZERO);
}

#[test]
fn wall_portals_never_funnel() {
    let set = pair(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
    let pull = set.funnel_displacement(Vec3::new(0.3, 0.0, 1.0), player_physics(), Vec3::ZERO, -10.0, 0.1);
    assert_eq!(pull, Vec3::ZERO);
}

// The user-facing promise of funneling: a hand-placed floor/ceiling pair
// with realistic misalignment loops indefinitely hands-off.
#[test]
fn misaligned_fall_loop_is_sustained_by_funneling() {
    let physics = player_physics();
    let layout = MapLayout {
        floors: vec![Floor {
            x1: -10.0,
            z1: -10.0,
            x2: 10.0,
            z2: 10.0,
            y: 0.0,
            thickness: FLOOR_THICKNESS,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout);
    let set = PortalSet::rebuild(
        &[
            portal(PortalEnd::A, Vec3::new(0.0, 0.0, 0.0), Vec3::Y, 0.0),
            portal(PortalEnd::B, Vec3::new(0.4, 4.0, 0.3), Vec3::NEG_Y, 0.0),
        ],
        &world,
        &Carriers::default(),
    );
    let env = CharacterEnvironment {
        ladder_mode: LadderMode::Automatic,
        collision_world: &world,
        gravity: 25.0,
        passable_kinds: &[],
        physics,
        ladder_climb_ratio: LADDER_CLIMB_RATIO,
        portals: Some(&set),
        carriers: &Carriers::default(),
    };

    let mut pos = Position { x: 0.0, y: 3.0, z: 0.0 };
    let mut vertical_velocity = 0.0_f32;
    let mut hops = 0;

    for _ in 0..(30 * 12) {
        let from = pos;
        let result = step_character_movement(
            CharacterStep {
                start: pos,
                vertical_velocity,
                control_velocity: Vec3::ZERO,
                external_displacement: Vec3::ZERO,
                delta: TICK_SECS,
            },
            &env,
        );
        pos = result.position;
        vertical_velocity = result.vertical_velocity;
        if let Some(hop) = set.character_hop(
            Vec3::from(from),
            Vec3::from(pos),
            physics,
            CharacterHopBody {
                control_velocity: Vec3::ZERO,
                knockback: Vec3::ZERO,
                airborne_momentum: Vec3::ZERO,
                vertical_velocity,
                yaw: 0.0,
            },
            22.5,
        ) {
            pos = hop.origin.into();
            vertical_velocity = hop.vertical_velocity;
            hops += 1;
        }
    }

    assert!(hops >= 15, "misaligned loop died after {hops} hops at {pos:?}");
}
