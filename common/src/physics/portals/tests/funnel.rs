use super::*;
use crate::constants::CHARACTER_TERMINAL_VELOCITY;

const GRAVITY: f32 = 25.0;

fn floor_around(x: f32, z: f32) -> Floor {
    Floor {
        x1: x - 10.0,
        z1: z - 10.0,
        x2: x + 10.0,
        z2: z + 10.0,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

fn ceiling_exit() -> Portal {
    portal(PortalEnd::B, Vec3::new(0.0, 4.0, 0.0), Vec3::NEG_Y, 0.0)
}

// Eight seconds of server ticks falling into a floor portal at the origin
// paired with `exit`, each tick the movement step (portal backing excluded,
// so the body sinks straight through) and then the crossing check between
// the previous and current positions. The body falls in hands-off and holds
// `steer` once the chain is running. Returns the fall speed at every entry
// and where the body ends.
fn fall_through_portals(floors: Vec<Floor>, exit: Portal, start_y: f32, steer: Vec3) -> (Vec<f32>, Position) {
    let world = CollisionWorld::from_map_layout(&MapLayout {
        floors,
        ..Default::default()
    });
    let carriers = Carriers::default();
    let entry = portal(PortalEnd::A, Vec3::ZERO, Vec3::Y, 0.0);
    let set = portal_set(&[entry, exit], &world, &carriers);
    let env = CharacterEnvironment {
        ladder_mode: LadderMode::Automatic,
        collision_world: &world,
        gravity: GRAVITY,
        passable_fields: &[],
        physics: player_physics(),
        portals: Some(&set),
        carriers: &carriers,
    };
    let mut pos = Position {
        x: 0.0,
        y: start_y,
        z: 0.0,
    };
    let mut vertical_velocity = 0.0_f32;
    let mut entry_speeds = Vec::new();
    for _ in 0..(30 * 8) {
        let control = if entry_speeds.is_empty() { Vec3::ZERO } else { steer };
        let from = pos;
        let result = step_character_movement(
            CharacterStep {
                start: pos,
                vertical_velocity,
                intent_velocity: control,
                velocity: control,
                displacement: Vec3::ZERO,
                delta: TICK_SECS,
            },
            &env,
        );
        pos = result.position;
        vertical_velocity = result.vertical_velocity;
        let body = hop_body(control, vertical_velocity, 0.0);
        if let Some(hop) = player_hop(&set, Vec3::from(from), Vec3::from(pos), body) {
            entry_speeds.push(-vertical_velocity);
            pos = hop.origin.into();
            vertical_velocity = hop.vertical_velocity;
        }
    }
    (entry_speeds, pos)
}

#[test]
fn perpetual_floor_fall_keeps_its_speed_across_hops() {
    let floors = vec![floor_around(0.0, 0.0), floor_around(50.0, 50.0)];
    let exit = portal(PortalEnd::B, Vec3::new(50.0, 0.0, 50.0), Vec3::Y, 0.0);
    let (entry_speeds, _) = fall_through_portals(floors, exit, 8.0, Vec3::ZERO);

    assert!(
        entry_speeds.len() >= 3,
        "only {} hops in 8 s: {entry_speeds:?}",
        entry_speeds.len()
    );
    let first = entry_speeds[0];
    let last = *entry_speeds.last().expect("no hops recorded");
    let expected = (2.0 * GRAVITY * 8.0).sqrt().min(CHARACTER_TERMINAL_VELOCITY);
    assert!(first > expected - 3.0, "first entry too slow: {entry_speeds:?}");
    assert!(last > first - 3.0, "speed decayed across hops: {entry_speeds:?}");
}

// The fast cycle: floor portal with its pair on the ceiling directly
// above. Every pass adds a room of gravity; speed must build to the
// terminal cap and stay there.
#[test]
fn floor_to_ceiling_fall_accelerates_toward_terminal_velocity() {
    let (entry_speeds, _) = fall_through_portals(vec![floor_around(0.0, 0.0)], ceiling_exit(), 3.0, Vec3::ZERO);

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
    let set = pair(Vec3::ZERO, Vec3::Y, Vec3::new(10.0, 4.0, 10.0), Vec3::NEG_Y);
    let falling = hop_body(Vec3::ZERO, -5.0, 0.0);
    let hop = player_hop(&set, Vec3::new(0.0, -0.85, 0.5), Vec3::new(0.0, -0.95, 0.5), falling)
        .expect("offset crossing did not trigger");
    assert!((hop.origin.x - 10.0).abs() < 1e-4);
    assert!((hop.origin.z - 10.5).abs() < 1e-4);
}

#[test]
fn carried_offset_is_clamped_to_the_exit_aperture() {
    let set = pair(Vec3::ZERO, Vec3::Y, Vec3::new(10.0, 4.0, 10.0), Vec3::NEG_Y);
    let falling = hop_body(Vec3::ZERO, -5.0, 0.0);
    let hop = player_hop(&set, Vec3::new(0.55, -0.85, 0.0), Vec3::new(0.55, -0.95, 0.0), falling)
        .expect("edge crossing did not trigger");
    let limit = PORTAL_HALF_WIDTH - player_physics().movement_collider.radius();
    assert!((hop.origin.x - 10.0).abs() <= limit + 1e-4);
    assert!(hop.origin.x > 10.0);
}

// Holding a direction while looping must break the loop within a few
// hops: the crossing gate is aperture-bound, so accumulated drift makes
// the body miss the hole and land beside it.
#[test]
fn steering_sideways_escapes_a_portal_fall_chain() {
    let steer = Vec3::new(0.0, 0.0, 6.0);
    let (entry_speeds, pos) = fall_through_portals(vec![floor_around(0.0, 0.0)], ceiling_exit(), 3.0, steer);

    let hops = entry_speeds.len();
    assert!(hops >= 1, "the chain never started");
    assert!(hops <= 10, "steering never escaped the chain: {hops} hops");
    assert!(pos.z > 2.0, "escaped body did not keep moving: z = {}", pos.z);
}
