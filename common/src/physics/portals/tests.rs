use std::{collections::HashMap, f32::consts::PI};

use bevy_math::Vec3;

use super::{traversal::traverse_yaw, *};
use crate::{
    config::{CharacterPhysicsConfig, KnockbackConfig, MapMovementConfig, PlayerMovementConfig, PortalShotSettings},
    constants::{
        PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH, PORTAL_LIGHT_CLEARANCE, PORTAL_PROJECTILE_EXIT_STANDOFF,
        PORTAL_RIM_SCALE, TICK_SECS,
    },
    map::{Carriers, carrier_offset_at},
    math::angle_delta_radians,
    physics::{
        AirborneMomentum, CharacterMovementResult, CharacterSupport, CharacterVerticalVelocity, CollisionWorld,
        KnockbackVelocity, ProjectileEvent, ProjectileMotion, earliest_projectile_event, momentum_displacement,
    },
    protocol::{
        Barrier, BarrierKindId, BarrierKindTable, BridgeKindId, Carrier, CarrierId, FaceYaw, Floor, LightBridge,
        MapLayout, PlatePurpose, PlayerMoveIntent, Portal, PortalEnd, PortalPairId, Position, PressurePlate, Ramp,
        Wall, WallLight,
    },
    test_geometry::{BARRIER_THICKNESS, BRIDGE_THICKNESS, FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS},
};

const CAP: f32 = 22.5;
const LADDER_CLIMB_RATIO: f32 = 0.4;

fn map_movement() -> MapMovementConfig {
    MapMovementConfig {
        player: PlayerMovementConfig {
            walk_speed: 6.0,
            run_speed: 9.0,
            speed_power_up: 1.6,
            jump_speed: 12.0,
        },
        actors: HashMap::new(),
        missile_speed: 16.0,
        projectile_speed: 90.0,
        gravity: 25.0,
        low_gravity: 5.0,
        ladder_climb_ratio: LADDER_CLIMB_RATIO,
        knockback: KnockbackConfig {
            max_speed: 15.0,
            up_speed: 7.0,
            deceleration: 35.0,
        },
    }
}

fn portal(end: PortalEnd, pos: Vec3, normal: Vec3, yaw: f32) -> Portal {
    Portal {
        pair: PortalPairId(1),
        end,
        pos: pos.into(),
        nx: normal.x,
        ny: normal.y,
        nz: normal.z,
        yaw,
        carrier: CarrierId::WORLD,
    }
}

fn empty_world() -> CollisionWorld {
    CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default())
}

fn pair(a_pos: Vec3, a_normal: Vec3, b_pos: Vec3, b_normal: Vec3) -> PortalSet {
    PortalSet::rebuild(
        &[
            portal(PortalEnd::A, a_pos, a_normal, 0.0),
            portal(PortalEnd::B, b_pos, b_normal, 0.0),
        ],
        &empty_world(),
        &Carriers::default(),
    )
}

fn frames(set: &PortalSet) -> (&PortalFrame, &PortalFrame) {
    set.first_pair_frames().expect("portal set has no pair")
}

fn player_physics() -> CharacterPhysicsConfig {
    crate::config::gameplay::load_test_gameplay()
        .expect("default gameplay config should load")
        .player
        .physics()
}

fn assert_frame_valid(frame: &PortalFrame) {
    assert!((frame.normal.length() - 1.0).abs() < 1e-5);
    assert!((frame.up.length() - 1.0).abs() < 1e-5);
    assert!((frame.right.length() - 1.0).abs() < 1e-5);
    assert!(frame.up.dot(frame.normal).abs() < 1e-5);
    assert!((frame.right.cross(frame.up) - frame.normal).length() < 1e-5);
}

#[test]
fn frames_are_right_handed_orthonormal_for_any_normal() {
    for normal in [
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Z,
        Vec3::NEG_Z,
        Vec3::Y,
        Vec3::NEG_Y,
        Vec3::new(0.0, 0.6, 0.8),
        Vec3::new(-1.0, -1.0, 1.4),
    ] {
        let frame = PortalFrame::from_portal(&portal(PortalEnd::A, Vec3::ZERO, normal, 1.2), &Carriers::default());
        assert_frame_valid(&frame);
    }
}

#[test]
fn ramp_frame_up_points_along_the_slope() {
    let frame = PortalFrame::from_portal(
        &portal(PortalEnd::A, Vec3::ZERO, Vec3::new(0.0, 0.6, 0.8), 0.0),
        &Carriers::default(),
    );
    assert!((frame.up - Vec3::new(0.0, 0.8, -0.6)).length() < 1e-5);
    assert!((frame.right - Vec3::X).length() < 1e-5);
}

#[test]
fn traversal_preserves_speed() {
    let set = pair(
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.6, 0.8),
        Vec3::new(9.0, 3.0, 1.0),
        Vec3::X,
    );
    let (entry, exit) = frames(&set);
    let v = Vec3::new(1.3, -4.2, 2.9);
    assert!((traverse_vector(entry, exit, v).length() - v.length()).abs() < 1e-4);
}

#[test]
fn facing_wall_pair_acts_as_a_tunnel() {
    let set = pair(
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::Z,
        Vec3::new(0.0, 1.0, 10.0),
        Vec3::NEG_Z,
    );
    let (entry, exit) = frames(&set);
    let v = Vec3::new(0.0, 0.0, -6.0);
    assert!((traverse_vector(entry, exit, v) - v).length() < 1e-5);
    assert!(angle_delta_radians(traverse_yaw(entry, exit, PI), PI).abs() < 1e-5);
}

#[test]
fn same_wall_pair_reverses_heading() {
    let set = pair(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, Vec3::new(5.0, 1.0, 0.0), Vec3::Z);
    let (entry, exit) = frames(&set);
    let out = traverse_vector(entry, exit, Vec3::new(0.0, 0.0, -6.0));
    assert!((out - Vec3::new(0.0, 0.0, 6.0)).length() < 1e-5);
    assert!(angle_delta_radians(traverse_yaw(entry, exit, PI), 0.0).abs() < 1e-5);
}

#[test]
fn same_wall_hop_maps_held_input_away_from_the_exit() {
    let set = pair(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, Vec3::new(5.0, 1.6, 0.0), Vec3::Z);
    let physics = player_physics();
    let intent = PlayerMoveIntent::Running { direction: PI };
    let control = intent.to_horizontal_velocity(2.0, 6.0, false, 1.0);
    let hop = set
        .character_hop(
            Vec3::new(0.0, 0.7, 0.15),
            Vec3::new(0.0, 0.7, -0.05),
            physics,
            control,
            Vec3::ZERO,
            Vec3::ZERO,
            0.0,
            PI,
            CAP,
        )
        .expect("same-wall entry did not hop");
    let mut position = Position::default();
    let mut face_yaw = FaceYaw(PI);
    let mut vertical_velocity = CharacterVerticalVelocity(0.0);
    let mut mapped = intent;
    hop.apply_player_state(&mut position, &mut face_yaw, &mut vertical_velocity, &mut mapped);
    assert_eq!(position, hop.origin.into());
    assert_eq!(face_yaw.0, hop.yaw);
    assert_eq!(vertical_velocity.0, hop.vertical_velocity);
    let mapped_direction = mapped.direction().expect("running intent became idle");
    assert!(angle_delta_radians(mapped_direction, 0.0).abs() < 1e-4);

    let next_control = mapped.to_horizontal_velocity(2.0, 6.0, false, 1.0);
    let next = hop.origin + next_control * 0.1;
    assert!((next - hop.exit.center).dot(hop.exit.normal) > (hop.origin - hop.exit.center).dot(hop.exit.normal));
    assert!(
        set.character_hop(
            hop.origin,
            next,
            physics,
            next_control,
            hop.knockback,
            hop.portal_momentum,
            hop.vertical_velocity,
            hop.yaw,
            CAP,
        )
        .is_none()
    );
}

#[test]
fn travelers_rightward_drift_stays_rightward() {
    // Facing -Z the traveler's right is +X; through a facing pair the
    // exit heading stays -Z, so their right must stay +X.
    let set = pair(
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::Z,
        Vec3::new(0.0, 1.0, 10.0),
        Vec3::NEG_Z,
    );
    let (entry, exit) = frames(&set);
    let out = traverse_vector(entry, exit, Vec3::new(0.5, 0.0, -6.0));
    assert!((out - Vec3::new(0.5, 0.0, -6.0)).length() < 1e-5);
    // Through a same-wall pair the exit heading is +Z: traveler's right
    // becomes world -X, and the drift must follow it.
    let set = pair(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, Vec3::new(5.0, 1.0, 0.0), Vec3::Z);
    let (entry, exit) = frames(&set);
    let out = traverse_vector(entry, exit, Vec3::new(0.5, 0.0, -6.0));
    assert!((out - Vec3::new(-0.5, 0.0, 6.0)).length() < 1e-5);
}

#[test]
fn wall_to_wall_yaw_matches_closed_form() {
    for (entry_normal, exit_normal) in [(Vec3::Z, Vec3::X), (Vec3::NEG_X, Vec3::Z), (Vec3::X, Vec3::NEG_Z)] {
        let set = pair(
            Vec3::new(0.0, 1.0, 0.0),
            entry_normal,
            Vec3::new(7.0, 1.0, 3.0),
            exit_normal,
        );
        let (entry, exit) = frames(&set);
        let yaw = entry_normal.x.atan2(entry_normal.z) + PI - 0.4; // mostly into the entry
        let expected = yaw + exit_normal.x.atan2(exit_normal.z) - entry_normal.x.atan2(entry_normal.z) + PI;
        assert!(angle_delta_radians(traverse_yaw(entry, exit, yaw), expected).abs() < 1e-4);
    }
}

#[test]
fn square_on_wall_entry_to_floor_exit_faces_the_exit_up() {
    let set = pair(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, Vec3::new(5.0, 0.0, 5.0), Vec3::Y);
    let (entry, exit) = frames(&set);
    // Walking dead-on into the wall maps the facing vertical; the fallback
    // is the floor exit's in-plane up (its placement yaw, here 0 = +Z).
    assert!(angle_delta_radians(traverse_yaw(entry, exit, PI), 0.0).abs() < 1e-4);
}

#[test]
fn falling_into_floor_portal_carries_out_of_wall_as_portal_momentum() {
    let set = pair(Vec3::new(0.0, 0.0, 0.0), Vec3::Y, Vec3::new(10.0, 2.0, 0.0), Vec3::X);
    let hop = set
        .character_hop(
            Vec3::new(0.0, -0.85, 0.0),
            Vec3::new(0.0, -0.95, 0.0),
            player_physics(),
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::ZERO,
            -10.0,
            0.0,
            CAP,
        )
        .expect("fall through a floor portal did not trigger");
    assert!(hop.vertical_velocity.abs() < 1e-4);
    assert!(hop.knockback.length() < 1e-4);
    assert!((hop.portal_momentum - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-4);
}

#[test]
fn walking_into_wall_portal_exits_floor_portal_upward() {
    let set = pair(Vec3::new(0.0, 0.9, 0.0), Vec3::Z, Vec3::new(10.0, 0.0, 10.0), Vec3::Y);
    let hop = set
        .character_hop(
            Vec3::new(0.0, 0.0, 0.1),
            Vec3::new(0.0, 0.0, -0.1),
            player_physics(),
            Vec3::new(0.0, 0.0, -6.0),
            Vec3::ZERO,
            Vec3::ZERO,
            0.0,
            PI,
            CAP,
        )
        .expect("walk through a wall portal did not trigger");
    // Control maps into the vertical write but not either momentum carry.
    assert!((hop.vertical_velocity - 6.0).abs() < 1e-4);
    assert!(hop.knockback.length() < 1e-4);
    assert!(hop.portal_momentum.length() < 1e-4);
    // Emerges half-in: the crossing penetration is carried through.
    assert!((hop.origin.y - (0.1 - 0.9)).abs() < 1e-4);
}

#[test]
fn falling_into_floor_portal_exits_ramp_at_its_normal_angle() {
    let ramp_normal = Vec3::new(0.0, 0.6, 0.8);
    let set = pair(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::Y,
        Vec3::new(10.0, 2.0, 10.0),
        ramp_normal,
    );
    let gameplay = crate::config::gameplay::load_test_gameplay().expect("default gameplay config should load");
    let movement = map_movement();
    let hop = set
        .player_hop(
            Vec3::new(0.0, -0.85, 0.0),
            Vec3::new(0.0, -0.95, 0.0),
            &gameplay,
            &movement,
            PlayerMoveIntent::Idle,
            false,
            false,
            None,
            None,
            -10.0,
            0.0,
        )
        .expect("floor-to-ramp portal crossing missing");
    let exit_velocity = hop.portal_momentum + hop.knockback + Vec3::Y * hop.vertical_velocity;

    assert!((exit_velocity - ramp_normal * 10.0).length() < 1e-4);
    assert!(hop.knockback.length() < 1e-4);
    assert!(hop.portal_momentum.z > 1.0);
}

#[test]
fn portal_momentum_does_not_decay_between_airborne_steps() {
    let momentum = AirborneMomentum(Vec3::new(3.0, 0.0, -6.0));

    assert_eq!(momentum.step(0.1), Vec3::new(0.3, 0.0, -0.6));
    assert_eq!(momentum.step(0.1), Vec3::new(0.3, 0.0, -0.6));
}

#[test]
fn shared_momentum_displacement_combines_blast_and_portal_velocity() {
    let knockback = KnockbackVelocity(Vec3::X * 2.0);
    let momentum = AirborneMomentum(Vec3::Z * 3.0);

    assert_eq!(
        momentum_displacement(Some(&knockback), Some(&momentum), 0.5),
        Vec3::new(1.0, 0.0, 1.5)
    );
}

#[test]
fn portal_momentum_ends_on_support_or_collision() {
    let airborne = CharacterMovementResult {
        position: Default::default(),
        vertical_velocity: 1.0,
        support: CharacterSupport::Airborne,
        blocked: false,
        floor_velocity: Vec3::ZERO,
        crushed: false,
    };
    let mut momentum = AirborneMomentum(Vec3::X);
    momentum.finish_step(&airborne);
    assert_eq!(momentum.0, Vec3::X);

    let mut landed = airborne;
    landed.support = CharacterSupport::Ground;
    momentum.finish_step(&landed);
    assert_eq!(momentum.0, Vec3::ZERO);

    let mut blocked = airborne;
    blocked.blocked = true;
    let mut momentum = AirborneMomentum(Vec3::X);
    momentum.finish_step(&blocked);
    assert_eq!(momentum.0, Vec3::ZERO);
}

#[test]
fn crossing_the_plane_triggers_and_carries_penetration() {
    let set = pair(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
    let hop = set
        .character_hop(
            Vec3::new(0.0, 0.7, 0.15),
            Vec3::new(0.0, 0.7, -0.05),
            player_physics(),
            Vec3::new(0.0, 0.0, -6.0),
            Vec3::ZERO,
            Vec3::ZERO,
            0.0,
            PI,
            CAP,
        )
        .expect("crossing did not trigger");
    // The exit continues in front of the paired plane by the same
    // penetration the entry reached — seamless pass-through.
    assert!((hop.origin.x - 10.05).abs() < 1e-4);
}

#[test]
fn approaching_without_crossing_does_not_trigger() {
    let set = pair(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
    let hop = set.character_hop(
        Vec3::new(0.0, 0.7, 0.5),
        Vec3::new(0.0, 0.7, 0.1),
        player_physics(),
        Vec3::new(0.0, 0.0, -6.0),
        Vec3::ZERO,
        Vec3::ZERO,
        0.0,
        PI,
        CAP,
    );
    assert!(hop.is_none());
}

#[test]
fn crossing_from_behind_does_not_trigger() {
    let set = pair(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
    let hop = set.character_hop(
        Vec3::new(0.0, 0.7, -0.2),
        Vec3::new(0.0, 0.7, 0.2),
        player_physics(),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::ZERO,
        Vec3::ZERO,
        0.0,
        0.0,
        CAP,
    );
    assert!(hop.is_none());
}

#[test]
fn crossing_outside_the_aperture_does_not_trigger() {
    let set = pair(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
    let hop = set.character_hop(
        Vec3::new(2.0, 0.7, 0.15),
        Vec3::new(2.0, 0.7, -0.05),
        player_physics(),
        Vec3::new(0.0, 0.0, -6.0),
        Vec3::ZERO,
        Vec3::ZERO,
        0.0,
        PI,
        CAP,
    );
    assert!(hop.is_none());
}

#[test]
fn off_center_crossing_uses_the_full_rectangle() {
    // Body center 0.7 below and 0.65 beside the portal center: the oval
    // would reject this; the rectangular character gate does not.
    let set = pair(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
    let hop = set.character_hop(
        Vec3::new(0.65, 0.0, 0.15),
        Vec3::new(0.65, 0.0, -0.05),
        player_physics(),
        Vec3::new(0.0, 0.0, -6.0),
        Vec3::ZERO,
        Vec3::ZERO,
        0.0,
        PI,
        CAP,
    );
    assert!(hop.is_some());
}

#[test]
fn knockback_carry_is_capped() {
    let set = pair(Vec3::new(0.0, 0.0, 0.0), Vec3::Y, Vec3::new(10.0, 2.0, 0.0), Vec3::X);
    let hop = set
        .character_hop(
            Vec3::new(0.0, -0.85, 0.0),
            Vec3::new(0.0, -0.95, 0.0),
            player_physics(),
            Vec3::ZERO,
            Vec3::X * 50.0,
            Vec3::ZERO,
            -1.0,
            0.0,
            CAP,
        )
        .expect("fall through a floor portal did not trigger");
    assert!((hop.knockback.length() - CAP).abs() < 1e-4);
}

#[test]
fn projectile_hop_requires_front_side_approach() {
    let set = pair(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
    let toward = set.projectile_hop(Vec3::new(0.0, 1.0, 2.0), Vec3::new(0.0, 0.0, -30.0), 0.1, 0.08);
    assert!(toward.is_some());
    let from_behind = set.projectile_hop(Vec3::new(0.0, 1.0, -2.0), Vec3::new(0.0, 0.0, 30.0), 0.1, 0.08);
    assert!(from_behind.is_none());
}

#[test]
fn projectile_hop_rejects_shots_outside_the_aperture() {
    let set = pair(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
    let hop = set.projectile_hop(
        Vec3::new(2.0 * PORTAL_HALF_WIDTH, 1.0, 2.0),
        Vec3::new(0.0, 0.0, -30.0),
        0.1,
        0.08,
    );
    assert!(hop.is_none());
}

#[test]
fn projectile_continues_straight_through_a_facing_pair() {
    let set = pair(
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::Z,
        Vec3::new(0.0, 1.0, 10.0),
        Vec3::NEG_Z,
    );
    let velocity = Vec3::new(1.0, 0.0, -30.0);
    let hop = set
        .projectile_hop(Vec3::new(0.2, 1.1, 2.0), velocity, 0.1, 0.08)
        .expect("projectile aimed at the portal did not hop");
    // A facing pair is a tunnel: the lateral offset and velocity carry over.
    assert!((hop.exit_velocity - velocity).length() < 1e-4);
    assert!((hop.exit_pos.x - hop.entry_point.x).abs() < 1e-4);
    assert!((hop.exit_pos.y - hop.entry_point.y).abs() < 1e-4);
    assert!((hop.exit_velocity.length() - velocity.length()).abs() < 1e-4);
}

fn moving_projectile_portals(entry_travel: Vec3, exit_travel: Vec3, obstacles: &[Wall]) -> (CollisionWorld, PortalSet) {
    let carrier = Carrier {
        parent: CarrierId::WORLD,
        level: 0,
        levels: 0,
        from: Position::default(),
        to: entry_travel.into(),
        travel_ticks: 1,
        pause_ticks: 0,
        phase_ticks: 0,
    };
    let wall = Wall {
        x1: -2.0,
        x2: 2.0,
        z1: -0.1,
        z2: -0.1,
        y: 0.0,
        height: 3.0,
        width: 0.2,
        level: 0,
        carrier: CarrierId(1),
    };
    let layout = MapLayout {
        carriers: vec![
            carrier,
            Carrier {
                from: (Vec3::X * 10.0).into(),
                to: (Vec3::X * 10.0 + exit_travel).into(),
                ..carrier
            },
        ],
        walls: [
            wall,
            Wall {
                carrier: CarrierId(2),
                ..wall
            },
        ]
        .into_iter()
        .chain(obstacles.iter().copied())
        .collect(),
        ..Default::default()
    };
    let (world, carriers) = tile_world(&layout, 1);
    let set = PortalSet::rebuild(
        &[
            Portal {
                carrier: CarrierId(1),
                ..portal(PortalEnd::A, Vec3::Y, Vec3::Z, 0.0)
            },
            Portal {
                carrier: CarrierId(2),
                ..portal(PortalEnd::B, Vec3::Y, Vec3::Z, 0.0)
            },
        ],
        &world,
        &carriers,
    );
    (world, set)
}

#[test]
fn an_approaching_portal_catches_a_projectile_it_sweeps_across() {
    let travel = Vec3::Z * 0.2;
    let (_, set) = moving_projectile_portals(travel, Vec3::ZERO, &[]);
    for speed in [-0.1, 0.0, 0.1] {
        let start = Vec3::new(0.0, 1.0, 0.15);
        let hop = set
            .projectile_hop(start, Vec3::Z * speed, TICK_SECS, 0.08)
            .expect("approaching portal missed the projectile");
        assert!((hop.entry_point.z - travel.z * hop.t - 0.08).abs() < 1e-5, "{hop:?}");
        assert!(
            hop.entry_point
                .abs_diff_eq(start + Vec3::Z * speed * TICK_SECS * hop.t, 1e-5)
        );
    }
}

#[test]
fn a_retreating_portal_does_not_catch_a_projectile_that_keeps_its_distance() {
    let travel = Vec3::NEG_Z * 0.2;
    let (_, set) = moving_projectile_portals(travel, Vec3::ZERO, &[]);
    assert!(
        set.projectile_hop(Vec3::new(0.0, 1.0, 0.15), travel / TICK_SECS, TICK_SECS, 0.08)
            .is_none()
    );
}

#[test]
fn a_sliding_portal_uses_its_aperture_at_the_crossing_time() {
    let travel = Vec3::X * (4.0 * PORTAL_HALF_WIDTH);
    let (_, set) = moving_projectile_portals(travel, Vec3::ZERO, &[]);
    let velocity = Vec3::NEG_Z * 6.0;
    let hop = set
        .projectile_hop(Vec3::new(travel.x / 2.0, 1.0, 0.18), velocity, TICK_SECS, 0.08)
        .expect("projectile missed the sliding aperture at mid-tick");
    assert!((hop.t - 0.5).abs() < 1e-5);
    assert!((hop.exit_pos.x - 10.0).abs() < 1e-5, "{hop:?}");
    assert!(
        set.projectile_hop(Vec3::new(0.0, 1.0, 0.18), velocity, TICK_SECS, 0.08)
            .is_none()
    );
}

#[test]
fn a_projectile_segment_uses_only_the_portals_remaining_tick_travel() {
    let (_, set) = moving_projectile_portals(Vec3::Z * 0.2, Vec3::ZERO, &[]);
    let hop = set
        .projectile_hop(Vec3::new(0.0, 1.0, 0.27), Vec3::ZERO, TICK_SECS / 4.0, 0.08)
        .expect("portal missed the projectile during the final quarter tick");
    assert!((hop.t - 0.8).abs() < 1e-5, "{hop:?}");
}

#[test]
fn a_projectile_emerges_at_the_moving_exits_crossing_time() {
    let travel = Vec3::new(0.4, 0.2, -0.1);
    let (_, set) = moving_projectile_portals(Vec3::ZERO, travel, &[]);
    let hop = set
        .projectile_hop(Vec3::new(0.0, 1.0, 0.18), Vec3::NEG_Z * 6.0, TICK_SECS, 0.08)
        .expect("projectile missed the static entry portal");
    assert!((hop.t - 0.5).abs() < 1e-5);
    let expected = Vec3::new(10.0, 1.0, 0.0) + travel * 0.5 + Vec3::Z * (0.08 + PORTAL_PROJECTILE_EXIT_STANDOFF);
    assert!(hop.exit_pos.abs_diff_eq(expected, 1e-5), "{hop:?}");
}

#[test]
fn an_approaching_portals_backing_wall_does_not_win_a_premature_bounce() {
    let (world, set) = moving_projectile_portals(Vec3::Z * 0.2, Vec3::ZERO, &[]);
    let start = Vec3::new(0.0, 1.0, 1.0);
    let velocity = Vec3::NEG_Z * 90.0;
    let hop = set
        .projectile_hop(start, velocity, TICK_SECS, 0.08)
        .expect("projectile missed the portal");
    let surface = world.cast_moving_ball_excluding(start, velocity * TICK_SECS, 0.08, hop.entry_backing);
    assert_eq!(
        earliest_projectile_event(None, None, surface.map(|hit| hit.t), Some(hop.t)),
        ProjectileEvent::Portal
    );

    let outside = Vec3::new(PORTAL_HALF_WIDTH * 2.0, 1.0, 1.0);
    assert!(set.projectile_hop(outside, velocity, TICK_SECS, 0.08).is_none());
    let surface = world.cast_moving_ball(outside, velocity * TICK_SECS, 0.08);
    assert_eq!(
        earliest_projectile_event(None, None, surface.map(|hit| hit.t), None),
        ProjectileEvent::Surface
    );
}

fn portal_test_projectile(velocity: Vec3) -> ProjectileMotion {
    let mut config = crate::config::gameplay::load_test_gameplay()
        .expect("test gameplay config invalid")
        .projectiles;
    config.radius = 0.08;
    config.bounce_retention = 1.0;
    ProjectileMotion::from_velocity(velocity, &config)
}

fn projectile_obstacle(z: f32, width: f32) -> Wall {
    Wall {
        x1: -2.0,
        x2: 2.0,
        z1: z,
        z2: z,
        y: 0.0,
        height: 3.0,
        width,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

#[test]
fn a_valid_portal_crossing_still_bounces_off_an_unrelated_obstacle() {
    let (world, set) = moving_projectile_portals(Vec3::Z * 0.2, Vec3::ZERO, &[projectile_obstacle(0.18, 0.02)]);
    let start = Position { x: 0.0, y: 1.0, z: 1.0 };
    let mut projectile = portal_test_projectile(Vec3::NEG_Z * 90.0);
    let hop = set
        .projectile_hop(start.into(), projectile.velocity, TICK_SECS, 0.08)
        .expect("projectile missed the portal");
    let surface_t = projectile.surface_collision_t(&start, TICK_SECS, &world, hop.entry_backing);
    assert_eq!(
        earliest_projectile_event(None, None, surface_t, Some(hop.t)),
        ProjectileEvent::Surface
    );

    let bounce = projectile
        .bounce_at_world_surface(&start, TICK_SECS, &world, hop.entry_backing)
        .expect("obstacle did not bounce the projectile");
    assert!((bounce.position.z - 0.27).abs() < 1e-4, "{bounce:?}");
    assert!(projectile.velocity.z > 0.0);
}

#[test]
fn a_bounced_projectile_crosses_a_moving_portal_during_the_same_tick() {
    let (world, set) = moving_projectile_portals(Vec3::Z * 0.2, Vec3::ZERO, &[projectile_obstacle(0.83, 0.2)]);
    let start = Position { x: 0.0, y: 1.0, z: 0.4 };
    let mut projectile = portal_test_projectile(Vec3::Z * 30.0);
    let bounce = projectile
        .bounce_at_world_surface(&start, TICK_SECS, &world, &[])
        .expect("projectile missed the bounce wall");
    assert!((bounce.remaining_delta / TICK_SECS - 0.75).abs() < 1e-4);

    let hop = set
        .projectile_hop(
            bounce.position.into(),
            projectile.velocity,
            bounce.remaining_delta,
            0.08,
        )
        .expect("bounced projectile missed the portal");
    let crossing_tick = 1.0 - bounce.remaining_delta / TICK_SECS * (1.0 - hop.t);
    assert!((hop.entry_point.z - 0.2 * crossing_tick - 0.08).abs() < 1e-4, "{hop:?}");
    let surface_t = projectile.surface_collision_t(&bounce.position, bounce.remaining_delta, &world, hop.entry_backing);
    assert_eq!(
        earliest_projectile_event(None, None, surface_t, Some(hop.t)),
        ProjectileEvent::Portal
    );
}

#[test]
fn half_placed_pair_is_inert() {
    let set = PortalSet::rebuild(
        &[portal(PortalEnd::A, Vec3::new(0.0, 1.0, 0.0), Vec3::Z, 0.0)],
        &empty_world(),
        &Carriers::default(),
    );
    assert!(set.is_empty());
    let hop = set.projectile_hop(Vec3::new(0.0, 1.0, 2.0), Vec3::new(0.0, 0.0, -30.0), 0.1, 0.08);
    assert!(hop.is_none());
}

// One 12 m wall along X at z = 0 (level 0) with the room floor on +Z.
fn placement_layout() -> MapLayout {
    MapLayout {
        walls: vec![Wall {
            x1: -6.0,
            z1: 0.0,
            x2: 6.0,
            z2: 0.0,
            width: WALL_THICKNESS,
            level: 0,
            y: 0.0,
            height: WALL_HEIGHT,
            carrier: CarrierId::WORLD,
        }],
        floors: vec![Floor {
            x1: -6.0,
            z1: 0.0,
            x2: 6.0,
            z2: 6.0,
            y: 0.0,
            thickness: FLOOR_THICKNESS,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    }
}

fn place(layout: &MapLayout, origin: Vec3, toward: Vec3, yaw: f32) -> Option<PortalPlacement> {
    let world = CollisionWorld::from_map_layout(layout, &BarrierKindTable::default());
    compute_portal_placement(
        origin,
        (toward - origin).normalize(),
        yaw,
        40.0,
        &world,
        layout,
        &Carriers::default(),
        Default::default(),
        &[],
    )
}

#[test]
fn placement_accepts_a_clear_wall_center() {
    let layout = placement_layout();
    let placement =
        place(&layout, Vec3::new(0.0, 1.6, 3.0), Vec3::new(0.0, 1.6, 0.0), PI).expect("clear wall center rejected");
    assert!((placement.normal - Vec3::Z).length() < 1e-4);
}

#[test]
fn opening_a_barrier_exposes_a_fitting_portal_surface_behind_it() {
    let mut layout = placement_layout();
    layout.barriers.push(Barrier {
        x1: -6.0,
        z1: 2.0,
        x2: 6.0,
        z2: 2.0,
        width: BARRIER_THICKNESS,
        y: 0.0,
        height: WALL_HEIGHT,
        level: 0,
        levels: 1,
        kind: BarrierKindId(0),
        carrier: CarrierId::WORLD,
    });
    let kinds = BarrierKindTable::from_ids(vec!["gate".into(), "other".into()]).expect("barrier catalog rejected");
    let world = CollisionWorld::from_map_layout(&layout, &kinds);
    for open in [vec![], vec![BarrierKindId(1)], vec![BarrierKindId(0)], vec![]] {
        let placement = compute_portal_placement(
            Vec3::new(0.0, 1.6, 4.0),
            Vec3::NEG_Z,
            0.0,
            10.0,
            &world,
            &layout,
            &Carriers::default(),
            PortalShotSettings {
                barriers_block: true,
                light_bridges_block: true,
                erasers_block: true,
            },
            &open,
        );
        assert_eq!(placement.is_some(), open.contains(&BarrierKindId(0)));
        if let Some(placement) = placement {
            assert!((placement.pos.z - WALL_THICKNESS / 2.0).abs() < 1e-4);
            assert!(placement.normal.abs_diff_eq(Vec3::Z, 1e-4));
        }
    }
}

#[test]
fn bridge_power_controls_portal_placement_on_the_floor_and_ceiling_beyond_it() {
    let floor = Floor {
        x1: -3.0,
        z1: -3.0,
        x2: 3.0,
        z2: 3.0,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
        carrier: CarrierId::WORLD,
    };
    let ceiling_y = 2.0 * LEVEL_HEIGHT;
    let layout = MapLayout {
        floors: vec![
            floor,
            Floor {
                y: ceiling_y,
                level: 2,
                ..floor
            },
        ],
        light_bridges: vec![LightBridge {
            x1: -3.0,
            z1: -3.0,
            x2: 3.0,
            z2: 3.0,
            y: LEVEL_HEIGHT,
            thickness: BRIDGE_THICKNESS,
            level: 1,
            kind: BridgeKindId(0),
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let mut world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    for powered in [false, true, false] {
        world.set_powered_bridges(if powered { &[BridgeKindId(0)] } else { &[] });
        for (origin_y, direction, surface_y) in [
            (LEVEL_HEIGHT + 1.5, Vec3::NEG_Y, 0.0),
            (LEVEL_HEIGHT - 1.5, Vec3::Y, ceiling_y - FLOOR_THICKNESS),
        ] {
            let placement = compute_portal_placement(
                Vec3::Y * origin_y,
                direction,
                0.0,
                20.0,
                &world,
                &layout,
                &Carriers::default(),
                PortalShotSettings {
                    barriers_block: true,
                    light_bridges_block: true,
                    erasers_block: true,
                },
                &[],
            );
            assert_eq!(placement.is_some(), !powered);
            if let Some(placement) = placement {
                assert!((placement.pos.y - surface_y).abs() < 1e-4);
                assert!(placement.normal.abs_diff_eq(-direction, 1e-4));
            }
        }
    }
}

#[test]
fn placement_rejects_overlap_with_another_portal() {
    let placement = PortalPlacement {
        pos: Vec3::new(0.0, 1.6, 0.0),
        normal: Vec3::Z,
        yaw: 0.0,
        carrier: CarrierId::WORLD,
    };
    let existing = [portal(PortalEnd::A, Vec3::new(0.5, 1.6, 0.0), Vec3::Z, 0.0)];
    assert!(portal_placement_overlaps(
        &placement,
        PortalPairId(2),
        PortalEnd::B,
        &existing,
        &Carriers::default()
    ));
}

#[test]
fn placement_allows_clear_space_and_replacing_its_own_end() {
    let placement = PortalPlacement {
        pos: Vec3::new(0.0, 1.6, 0.0),
        normal: Vec3::Z,
        yaw: 0.0,
        carrier: CarrierId::WORLD,
    };
    let clear = [portal(PortalEnd::A, Vec3::new(2.0, 1.6, 0.0), Vec3::Z, 0.0)];
    assert!(!portal_placement_overlaps(
        &placement,
        PortalPairId(2),
        PortalEnd::B,
        &clear,
        &Carriers::default()
    ));

    let replaced = [portal(PortalEnd::B, Vec3::new(0.0, 1.6, 0.0), Vec3::Z, 0.0)];
    assert!(!portal_placement_overlaps(
        &placement,
        PortalPairId(1),
        PortalEnd::B,
        &replaced,
        &Carriers::default()
    ));
}

#[test]
fn swept_portal_gate_uses_the_plane_crossing_point() {
    let layout = placement_layout();
    let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let placement =
        place(&layout, Vec3::new(0.0, 1.6, 3.0), Vec3::new(0.0, 1.6, 0.0), PI).expect("clear wall center rejected");
    let set = PortalSet::rebuild(
        &[
            portal(PortalEnd::A, placement.pos, placement.normal, placement.yaw),
            portal(PortalEnd::B, Vec3::new(10.0, 1.6, 10.0), Vec3::X, 0.0),
        ],
        &world,
        &Carriers::default(),
    );
    let physics = player_physics();
    let inside_from = Vec3::new(0.4, 0.7, placement.pos.z + 0.15);
    let inside_move = Vec3::new(0.4, 0.0, -0.4);
    let inside_to = inside_from + inside_move;
    assert!(
        !set.movement_collision_exclusions(inside_from, inside_move, physics)
            .is_empty()
    );
    assert!(
        set.character_hop(
            inside_from,
            inside_to,
            physics,
            inside_move,
            Vec3::ZERO,
            Vec3::ZERO,
            0.0,
            PI,
            CAP,
        )
        .is_some()
    );

    let outside_from = Vec3::new(0.65, 0.7, placement.pos.z + 0.15);
    let outside_move = Vec3::new(0.3, 0.0, -0.4);
    let outside_to = outside_from + outside_move;
    assert!(
        set.movement_collision_exclusions(outside_from, outside_move, physics)
            .is_empty()
    );
    assert!(
        set.character_hop(
            outside_from,
            outside_to,
            physics,
            outside_move,
            Vec3::ZERO,
            Vec3::ZERO,
            0.0,
            PI,
            CAP,
        )
        .is_none()
    );
}

#[test]
fn low_wall_shot_nudges_up_until_the_aperture_fits() {
    let layout = placement_layout();
    let placement = place(&layout, Vec3::new(0.0, 1.6, 3.0), Vec3::new(0.0, 0.5, 0.0), PI)
        .expect("low shot did not nudge up onto the wall");
    assert!(placement.pos.y > 1.3);
    assert!(placement.pos.x.abs() < 0.3);
}

#[test]
fn high_wall_shot_nudges_until_the_visible_rim_has_backing() {
    let layout = placement_layout();
    let placement = place(&layout, Vec3::new(0.0, 2.7, 3.0), Vec3::new(0.0, 2.7, 0.0), PI)
        .expect("high wall shot did not nudge below the wall top");

    assert!(placement.pos.y + PORTAL_HALF_HEIGHT * PORTAL_RIM_SCALE < WALL_HEIGHT);
}

#[test]
fn ramp_side_portal_rim_can_meet_the_slope() {
    let ramp_length = 6.0;
    let slope = LEVEL_HEIGHT / ramp_length;
    let z = 1.5;
    let surface_y = slope * z;
    let rim_half_height = PORTAL_HALF_HEIGHT * PORTAL_RIM_SCALE;
    let rim_half_width = PORTAL_HALF_WIDTH * PORTAL_RIM_SCALE;
    let ellipse_support = (rim_half_height.powi(2) + (slope * rim_half_width).powi(2)).sqrt();
    let center_y = surface_y + ellipse_support + 0.01;
    let layout = MapLayout {
        walls: vec![Wall {
            x1: -2.0,
            z1: 0.0,
            x2: -2.0,
            z2: ramp_length,
            width: WALL_THICKNESS,
            level: 0,
            y: 0.0,
            height: WALL_HEIGHT,
            carrier: CarrierId::WORLD,
        }],
        ramps: vec![Ramp {
            x1: -2.0,
            y1: 0.0,
            z1: 0.0,
            x2: 2.0,
            y2: LEVEL_HEIGHT,
            z2: ramp_length,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let placement = place(&layout, Vec3::new(0.0, center_y, z), Vec3::new(-2.0, center_y, z), 0.0)
        .expect("ramp-side portal placement failed");

    assert!((placement.pos.y - center_y).abs() < 0.03, "{placement:?}");
    assert!((placement.pos.z - z).abs() < 0.03, "{placement:?}");
}

#[test]
fn wall_portal_near_ramp_excludes_only_wall_backing() {
    let ramp_length = 6.0;
    let slope = LEVEL_HEIGHT / ramp_length;
    let z = 1.5;
    let rim_half_height = PORTAL_HALF_HEIGHT * PORTAL_RIM_SCALE;
    let rim_half_width = PORTAL_HALF_WIDTH * PORTAL_RIM_SCALE;
    let ellipse_support = (rim_half_height.powi(2) + (slope * rim_half_width).powi(2)).sqrt();
    let center_y = slope * z + ellipse_support + 0.01;
    let layout = MapLayout {
        walls: vec![Wall {
            x1: -2.0,
            z1: 0.0,
            x2: -2.0,
            z2: ramp_length,
            width: WALL_THICKNESS,
            level: 0,
            y: 0.0,
            height: WALL_HEIGHT,
            carrier: CarrierId::WORLD,
        }],
        ramps: vec![Ramp {
            x1: -2.0,
            y1: 0.0,
            z1: 0.0,
            x2: 2.0,
            y2: LEVEL_HEIGHT,
            z2: ramp_length,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let set = PortalSet::rebuild(
        &[
            portal(PortalEnd::A, Vec3::new(-1.85, center_y, z), Vec3::X, 0.0),
            portal(PortalEnd::B, Vec3::new(10.0, 1.6, 10.0), Vec3::Z, 0.0),
        ],
        &world,
        &Carriers::default(),
    );
    let physics = player_physics();
    let origin = Vec3::new(-1.5, center_y - physics.collider.top_y_offset() / 2.0, z);

    assert_eq!(set.collision_exclusions(origin, physics).len(), 1);
}

#[test]
fn wall_portal_across_a_stacked_wall_opens_its_trim_strip() {
    let wall = |level: u8, y: f32| Wall {
        x1: -3.0,
        z1: 0.0,
        x2: 3.0,
        z2: 0.0,
        width: WALL_THICKNESS,
        level,
        y,
        height: WALL_HEIGHT,
        carrier: CarrierId::WORLD,
    };
    let layout = MapLayout {
        walls: vec![wall(0, 0.0), wall(1, LEVEL_HEIGHT)],
        floors: vec![Floor {
            x1: -3.0,
            z1: -WALL_THICKNESS / 2.0,
            x2: 3.0,
            z2: WALL_THICKNESS / 2.0,
            y: LEVEL_HEIGHT,
            thickness: FLOOR_THICKNESS,
            level: 1,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let set = PortalSet::rebuild(
        &[
            portal(
                PortalEnd::A,
                Vec3::new(0.0, LEVEL_HEIGHT, -WALL_THICKNESS / 2.0),
                -Vec3::Z,
                0.0,
            ),
            portal(PortalEnd::B, Vec3::new(10.0, 1.6, 10.0), Vec3::Z, 0.0),
        ],
        &world,
        &Carriers::default(),
    );
    let physics = player_physics();
    let origin = Vec3::new(0.0, LEVEL_HEIGHT - physics.collider.top_y_offset() / 2.0, -0.5);

    assert_eq!(set.collision_exclusions(origin, physics).len(), 3);
}

#[test]
fn wall_portal_keeps_the_floor_it_stands_on_solid() {
    let layout = MapLayout {
        walls: vec![Wall {
            x1: -3.0,
            z1: 0.0,
            x2: 3.0,
            z2: 0.0,
            width: WALL_THICKNESS,
            level: 0,
            y: 0.0,
            height: WALL_HEIGHT,
            carrier: CarrierId::WORLD,
        }],
        floors: vec![Floor {
            x1: -4.0,
            z1: -4.0,
            x2: 4.0,
            z2: 4.0,
            y: 0.0,
            thickness: FLOOR_THICKNESS,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let set = PortalSet::rebuild(
        &[
            portal(PortalEnd::A, Vec3::new(0.0, 1.0, -WALL_THICKNESS / 2.0), -Vec3::Z, 0.0),
            portal(PortalEnd::B, Vec3::new(10.0, 1.6, 10.0), Vec3::Z, 0.0),
        ],
        &world,
        &Carriers::default(),
    );
    let physics = player_physics();
    let origin = Vec3::new(0.0, 1.0 - physics.collider.top_y_offset() / 2.0, -0.5);

    assert_eq!(set.collision_exclusions(origin, physics).len(), 1);
}

#[test]
fn shot_past_the_walls_end_nudges_back_onto_it() {
    let layout = placement_layout();
    let placement = place(&layout, Vec3::new(5.9, 1.6, 3.0), Vec3::new(5.9, 1.6, 0.0), PI)
        .expect("edge shot did not nudge back onto the wall");
    assert!(placement.pos.x < 5.45);
}

#[test]
fn ramp_lip_shot_nudges_the_whole_aperture_onto_the_slope() {
    let ramp_length = 6.0;
    let layout = MapLayout {
        ramps: vec![Ramp {
            x1: -2.0,
            y1: 0.0,
            z1: 0.0,
            x2: 2.0,
            y2: LEVEL_HEIGHT,
            z2: ramp_length,
            carrier: CarrierId::WORLD,
        }],
        floors: vec![Floor {
            x1: -4.0,
            z1: ramp_length,
            x2: 4.0,
            z2: 12.0,
            y: LEVEL_HEIGHT,
            thickness: FLOOR_THICKNESS,
            level: 1,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let slope = LEVEL_HEIGHT / ramp_length;
    let target = Vec3::new(0.0, slope * 5.05, 5.05);
    let normal = Vec3::new(0.0, 1.0, -slope).normalize();
    let placement = place(&layout, target + normal * 3.0, target, 0.0).expect("ramp-lip shot did not nudge");
    let frame = PortalFrame::from_surface(placement.pos, placement.normal, placement.yaw);

    assert!((frame.center + frame.up * PORTAL_HALF_HEIGHT).z <= ramp_length);
}

#[test]
fn floor_shot_under_a_crossing_wall_nudges_clear_of_it() {
    let mut layout = placement_layout();
    // A second wall crossing the floor at z = 3.2 cuts any aperture that
    // straddles it — including between rim probes.
    layout.walls.push(Wall {
        x1: -6.0,
        z1: 3.2,
        x2: 6.0,
        z2: 3.2,
        width: WALL_THICKNESS,
        level: 0,
        y: 0.0,
        height: WALL_HEIGHT,
        carrier: CarrierId::WORLD,
    });
    let placement = place(&layout, Vec3::new(2.0, 1.6, 2.5), Vec3::new(2.0, 0.0, 2.5), 0.0)
        .expect("wall-cut floor shot did not nudge clear");
    assert!(placement.pos.z < 1.68);
    assert!((placement.pos.x - 2.0).abs() < 0.01);
}

#[test]
fn shot_with_no_fitting_spot_anywhere_fizzles() {
    let mut layout = placement_layout();
    // A 1 m stub wall can never back the 1.4 m aperture, nudged or not.
    layout.walls[0].x1 = -0.5;
    layout.walls[0].x2 = 0.5;
    let placement = place(&layout, Vec3::new(0.0, 1.6, 3.0), Vec3::new(0.0, 1.6, 0.0), PI);
    assert!(placement.is_none());
}

#[test]
fn shot_at_a_wall_light_nudges_clear_of_it() {
    let mut layout = placement_layout();
    layout.wall_lights.push(WallLight {
        pos: Position { x: 0.0, y: 1.6, z: 0.2 },
        yaw: 0.0,
        carrier: CarrierId::WORLD,
    });
    let placement = place(&layout, Vec3::new(0.0, 1.6, 3.0), Vec3::new(0.0, 1.6, 0.0), PI)
        .expect("shot at the light did not nudge clear");
    // The nudged aperture leaves the light outside its grown keep-out oval.
    let across = placement.pos.x / (PORTAL_HALF_WIDTH + PORTAL_LIGHT_CLEARANCE);
    let along_up = (placement.pos.y - 1.6) / (PORTAL_HALF_HEIGHT + PORTAL_LIGHT_CLEARANCE);
    assert!(across * across + along_up * along_up > 0.99);
    // Far enough along the wall the light does not even nudge the shot.
    let clear = place(&layout, Vec3::new(4.0, 1.6, 3.0), Vec3::new(4.0, 1.6, 0.0), PI).expect("clear shot rejected");
    assert!((clear.pos.x - 4.0).abs() < 0.01);
}

#[test]
fn wall_light_on_the_other_face_does_not_block_placement() {
    let mut layout = placement_layout();
    layout.wall_lights.push(WallLight {
        pos: Position {
            x: 0.0,
            y: 1.6,
            z: -0.17,
        },
        yaw: PI,
        carrier: CarrierId::WORLD,
    });
    let placement = place(&layout, Vec3::new(0.0, 1.6, 3.0), Vec3::new(0.0, 1.6, 0.0), PI)
        .expect("opposite-face light rejected placement");

    assert!(placement.pos.x.abs() < 0.01);
    assert!((placement.pos.y - 1.6).abs() < 0.01);
}

// A powered bridge slab crossing the oval is as solid as a floor to a
// traveller, so front clearance must see it; backing stays bridge-blind.
#[test]
fn placement_front_clearance_rejects_a_powered_light_bridge() {
    use crate::{
        constants::{PORTAL_HALF_HEIGHT, PORTAL_RIM_SCALE},
        protocol::{BridgeKindId, LightBridge},
        test_geometry::BRIDGE_THICKNESS,
    };

    let mut layout = placement_layout();
    layout.walls.push(Wall {
        x1: -6.0,
        z1: 0.0,
        x2: 6.0,
        z2: 0.0,
        width: WALL_THICKNESS,
        level: 1,
        y: LEVEL_HEIGHT,
        height: WALL_HEIGHT,
        carrier: CarrierId::WORLD,
    });
    layout.light_bridges.push(LightBridge {
        x1: -6.0,
        z1: 0.0,
        x2: 6.0,
        z2: 4.0,
        y: LEVEL_HEIGHT,
        level: 1,
        kind: BridgeKindId(0),
        thickness: BRIDGE_THICKNESS,
        carrier: CarrierId::WORLD,
    });
    let mut world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let origin = Vec3::new(0.0, 3.7, 3.0);
    let aim = Vec3::new(0.0, 3.7, 0.0);
    let shoot = |world: &CollisionWorld| {
        compute_portal_placement(
            origin,
            (aim - origin).normalize(),
            PI,
            40.0,
            world,
            &layout,
            &Carriers::default(),
            Default::default(),
            &[],
        )
    };

    let ghost = shoot(&world).expect("an unpowered bridge blocked the shot");
    assert!(
        (ghost.pos.y - aim.y).abs() < 1e-3,
        "ghost bridge moved the portal to {ghost:?}"
    );

    world.set_powered_bridges(&[BridgeKindId(0)]);
    let solid = shoot(&world).expect("no fitting spot below the powered bridge");
    let rim_top = solid.pos.y + PORTAL_HALF_HEIGHT * PORTAL_RIM_SCALE;
    assert!(
        rim_top <= LEVEL_HEIGHT - BRIDGE_THICKNESS,
        "portal rim at {rim_top} still crosses the powered bridge"
    );
}

#[test]
fn placement_rejects_a_floor_portal_covering_a_pressure_plate() {
    let mut layout = placement_layout();
    layout.pressure_plates.push(PressurePlate {
        level: 0,
        center_x: 3.0,
        center_z: 3.0,
        purpose: PlatePurpose::Firework,
        center_y: 0.0,
        carrier: CarrierId::WORLD,
    });
    assert!(place(&layout, Vec3::new(3.0, 1.6, 3.0), Vec3::new(3.0, 0.0, 3.0), 0.0).is_none());
    // The same shot well away from the plate lands.
    assert!(place(&layout, Vec3::new(-3.0, 1.6, 3.0), Vec3::new(-3.0, 0.0, 3.0), 0.0).is_some());
}

// Mirrors one server tick: the movement step (portal backing excluded,
// so the body sinks straight through), then the crossing check between
// the previous and current positions.
#[test]
fn perpetual_floor_fall_keeps_its_speed_across_hops() {
    use crate::{
        constants::TICK_SECS,
        physics::{CharacterEnvironment, CharacterStep, step_character_movement},
    };

    let gameplay = crate::config::gameplay::load_test_gameplay().expect("default gameplay config should load");
    let physics = gameplay.player.physics();
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
    let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let set = PortalSet::rebuild(
        &[
            portal(PortalEnd::A, Vec3::new(0.0, 0.0, 0.0), Vec3::Y, 0.0),
            portal(PortalEnd::B, Vec3::new(50.0, 0.0, 50.0), Vec3::Y, 0.0),
        ],
        &world,
        &Carriers::default(),
    );
    let env = CharacterEnvironment {
        collision_world: &world,
        gravity: 25.0,
        passable_kinds: &[],
        physics,
        ladder_climb_ratio: LADDER_CLIMB_RATIO,
        portals: Some(&set),
        carriers: &Carriers::default(),
    };

    let mut pos = crate::protocol::Position { x: 0.0, y: 8.0, z: 0.0 };
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
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::ZERO,
            vertical_velocity,
            0.0,
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
    let expected = (2.0 * env.gravity * 8.0)
        .sqrt()
        .min(crate::constants::CHARACTER_TERMINAL_VELOCITY);
    assert!(first > expected - 3.0, "first entry too slow: {entry_speeds:?}");
    assert!(last > first - 3.0, "speed decayed across hops: {entry_speeds:?}");
}

// The fast cycle: floor portal with its pair on the ceiling directly
// above. Every pass adds a room of gravity; speed must build to the
// terminal cap and stay there.
#[test]
fn floor_to_ceiling_fall_accelerates_toward_terminal_velocity() {
    use crate::{
        constants::TICK_SECS,
        physics::{CharacterEnvironment, CharacterStep, step_character_movement},
    };

    let gameplay = crate::config::gameplay::load_test_gameplay().expect("default gameplay config should load");
    let physics = gameplay.player.physics();
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
    let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let set = PortalSet::rebuild(
        &[
            portal(PortalEnd::A, Vec3::new(0.0, 0.0, 0.0), Vec3::Y, 0.0),
            portal(PortalEnd::B, Vec3::new(0.0, 4.0, 0.0), Vec3::NEG_Y, 0.0),
        ],
        &world,
        &Carriers::default(),
    );
    let env = CharacterEnvironment {
        collision_world: &world,
        gravity: 25.0,
        passable_kinds: &[],
        physics,
        ladder_climb_ratio: LADDER_CLIMB_RATIO,
        portals: Some(&set),
        carriers: &Carriers::default(),
    };

    let mut pos = crate::protocol::Position { x: 0.0, y: 3.0, z: 0.0 };
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
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::ZERO,
            vertical_velocity,
            0.0,
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
        last > crate::constants::CHARACTER_TERMINAL_VELOCITY - 2.0,
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
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::ZERO,
            -5.0,
            0.0,
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
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::ZERO,
            -5.0,
            0.0,
            CAP,
        )
        .expect("edge crossing did not trigger");
    let limit = PORTAL_HALF_WIDTH - physics.collider.width / 2.0;
    assert!((hop.origin.x - 10.0).abs() <= limit + 1e-4);
    assert!(hop.origin.x > 10.0);
}

// Holding a direction while looping must break the loop within a few
// hops — the whole point of carrying the aperture offset.
// Holding a direction while looping must break the loop within a few
// hops: the crossing gate is aperture-bound, so accumulated drift makes
// the body miss the hole and land beside it.
#[test]
fn steering_sideways_escapes_a_portal_fall_chain() {
    use crate::{
        constants::TICK_SECS,
        physics::{CharacterEnvironment, CharacterStep, step_character_movement},
    };

    let gameplay = crate::config::gameplay::load_test_gameplay().expect("default gameplay config should load");
    let physics = gameplay.player.physics();
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
    let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let set = PortalSet::rebuild(
        &[
            portal(PortalEnd::A, Vec3::new(0.0, 0.0, 0.0), Vec3::Y, 0.0),
            portal(PortalEnd::B, Vec3::new(0.0, 4.0, 0.0), Vec3::NEG_Y, 0.0),
        ],
        &world,
        &Carriers::default(),
    );
    let env = CharacterEnvironment {
        collision_world: &world,
        gravity: 25.0,
        passable_kinds: &[],
        physics,
        ladder_climb_ratio: LADDER_CLIMB_RATIO,
        portals: Some(&set),
        carriers: &Carriers::default(),
    };

    let mut pos = crate::protocol::Position { x: 0.0, y: 3.0, z: 0.0 };
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
            control,
            Vec3::ZERO,
            Vec3::ZERO,
            vertical_velocity,
            0.0,
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
fn an_external_teleport_is_not_a_crossing() {
    // Sign-crosses the plane, but no tick of real motion jumps this far.
    let set = pair(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
    let hop = set.character_hop(
        Vec3::new(0.0, 50.0, 0.15),
        Vec3::new(0.0, 0.7, -0.05),
        player_physics(),
        Vec3::ZERO,
        Vec3::ZERO,
        Vec3::ZERO,
        0.0,
        PI,
        CAP,
    );
    assert!(hop.is_none());
}

#[test]
fn vertical_placement_yaw_snaps_to_quarter_turns() {
    let layout = placement_layout();
    let floor = place(&layout, Vec3::new(-3.0, 1.6, 3.0), Vec3::new(-3.0, 0.0, 3.0), 1.0).expect("floor shot rejected");
    assert!((floor.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
    let wall = place(&layout, Vec3::new(0.0, 1.6, 3.0), Vec3::new(0.0, 1.6, 0.0), 1.0).expect("wall shot rejected");
    assert!((wall.yaw - 1.0).abs() < 1e-4);
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
    use crate::{
        constants::TICK_SECS,
        physics::{CharacterEnvironment, CharacterStep, step_character_movement},
    };

    let gameplay = crate::config::gameplay::load_test_gameplay().expect("default gameplay config should load");
    let physics = gameplay.player.physics();
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
    let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let set = PortalSet::rebuild(
        &[
            portal(PortalEnd::A, Vec3::ZERO, Vec3::Y, 0.0),
            portal(PortalEnd::B, Vec3::new(8.0, 4.0, 8.0), Vec3::NEG_Y, 0.0),
        ],
        &world,
        &Carriers::default(),
    );
    let env = CharacterEnvironment {
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
    use crate::{
        constants::TICK_SECS,
        physics::{CharacterEnvironment, CharacterStep, step_character_movement},
    };

    let gameplay = crate::config::gameplay::load_test_gameplay().expect("default gameplay config should load");
    let physics = gameplay.player.physics();
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
    let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let set = PortalSet::rebuild(
        &[
            portal(PortalEnd::A, Vec3::new(0.0, 0.0, 0.0), Vec3::Y, 0.0),
            portal(PortalEnd::B, Vec3::new(0.4, 4.0, 0.3), Vec3::NEG_Y, 0.0),
        ],
        &world,
        &Carriers::default(),
    );
    let env = CharacterEnvironment {
        collision_world: &world,
        gravity: 25.0,
        passable_kinds: &[],
        physics,
        ladder_climb_ratio: LADDER_CLIMB_RATIO,
        portals: Some(&set),
        carriers: &Carriers::default(),
    };

    let mut pos = crate::protocol::Position { x: 0.0, y: 3.0, z: 0.0 };
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
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::ZERO,
            vertical_velocity,
            0.0,
            22.5,
        ) {
            pos = hop.origin.into();
            vertical_velocity = hop.vertical_velocity;
            hops += 1;
        }
    }

    assert!(hops >= 15, "misaligned loop died after {hops} hops at {pos:?}");
}

// === Portals on carriers ===

const TILE: CarrierId = CarrierId(1);

// A slider tile at the origin travelling +X at 2 m/s, and a wall with a
// floor in front of it for the other end. `beside_floor` adds a static
// floor a hand's width past the tile's +X edge at rest.
fn tile_wall_layout(beside_floor: bool) -> MapLayout {
    use crate::test_geometry::{FLOOR_THICKNESS, WALL_THICKNESS};

    let mut layout = MapLayout {
        walls: vec![Wall {
            x1: -6.0,
            z1: -10.0,
            x2: 6.0,
            z2: -10.0,
            width: WALL_THICKNESS,
            level: 0,
            y: 0.0,
            height: WALL_HEIGHT,
            carrier: CarrierId::WORLD,
        }],
        floors: vec![
            Floor {
                x1: -6.0,
                z1: -10.0,
                x2: 6.0,
                z2: -4.0,
                y: 0.0,
                thickness: FLOOR_THICKNESS,
                level: 0,
                carrier: CarrierId::WORLD,
            },
            Floor {
                x1: -1.5,
                z1: -1.5,
                x2: 1.5,
                z2: 1.5,
                y: 0.0,
                thickness: FLOOR_THICKNESS,
                level: 0,
                carrier: TILE,
            },
        ],
        carriers: vec![Carrier {
            parent: CarrierId::WORLD,
            level: 0,
            levels: 0,
            from: Position::default(),
            to: Position { x: 4.0, y: 0.0, z: 0.0 },
            travel_ticks: 60,
            pause_ticks: 0,
            phase_ticks: 0,
        }],
        ..Default::default()
    };
    if beside_floor {
        layout.floors.push(Floor {
            x1: 1.6,
            z1: -3.0,
            x2: 8.0,
            z2: 3.0,
            y: 0.0,
            thickness: FLOOR_THICKNESS,
            level: 0,
            carrier: CarrierId::WORLD,
        });
    }
    layout
}

// The world with the tile at `tick`, its collider placed there.
fn tile_world(layout: &MapLayout, tick: u32) -> (CollisionWorld, Carriers) {
    let mut world = CollisionWorld::from_map_layout(layout, &BarrierKindTable::default());
    let mut carriers = Carriers::from_layout(layout);
    carriers.advance(tick.wrapping_sub(1));
    carriers.advance(tick);
    world.set_carrier_poses(&carriers);
    (world, carriers)
}

fn advance_tile(world: &mut CollisionWorld, carriers: &mut Carriers, set: &mut PortalSet, tick: u32) {
    carriers.advance(tick);
    world.set_carrier_poses(carriers);
    set.refresh(carriers);
}

fn tile_center(carriers: &Carriers) -> Vec3 {
    carriers.pose(TILE).translation
}

// A floor portal on the tile, at the carrier's origin; yaw 0 lays its long
// axis across the slide (along Z), a quarter turn along it.
fn carried_portal(yaw: f32) -> Portal {
    Portal {
        carrier: TILE,
        ..portal(PortalEnd::A, Vec3::ZERO, Vec3::Y, yaw)
    }
}

fn wall_portal() -> Portal {
    portal(PortalEnd::B, Vec3::new(0.0, 1.6, -9.85), Vec3::Z, 0.0)
}

// Runs the shared step from `first_tick` for `ticks`, the tile advancing
// and the gates refreshing before each, and stops at the first hop.
fn run_ticks(
    world: &mut CollisionWorld,
    carriers: &mut Carriers,
    set: &mut PortalSet,
    physics: CharacterPhysicsConfig,
    mut pos: Position,
    first_tick: u32,
    ticks: u32,
) -> (Option<(u32, CharacterPortalHop)>, Position, CharacterSupport) {
    use crate::{
        constants::TICK_SECS,
        physics::{CharacterEnvironment, CharacterStep, step_character_movement},
    };

    let mut vertical_velocity = 0.0;
    let mut support = CharacterSupport::Airborne;
    for tick in first_tick..first_tick + ticks {
        advance_tile(world, carriers, set, tick);
        let env = CharacterEnvironment {
            collision_world: world,
            gravity: 25.0,
            passable_kinds: &[],
            physics,
            ladder_climb_ratio: LADDER_CLIMB_RATIO,
            portals: Some(set),
            carriers,
        };
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
        support = result.support;
        if let Some(hop) = set.character_hop(
            Vec3::from(from),
            Vec3::from(pos),
            physics,
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::ZERO,
            vertical_velocity,
            0.0,
            CAP,
        ) {
            return (Some((tick, hop)), pos, support);
        }
    }
    (None, pos, support)
}

#[test]
fn a_shot_at_a_carrier_floor_places_the_portal_on_the_carrier() {
    let layout = tile_wall_layout(false);
    let (world, carriers) = tile_world(&layout, 1);

    let placement = compute_portal_placement(
        Vec3::new(0.3, 3.0, 0.0),
        Vec3::NEG_Y,
        0.0,
        40.0,
        &world,
        &layout,
        &carriers,
        Default::default(),
        &[],
    )
    .expect("a shot at the tile fizzled");

    assert_eq!(placement.carrier, TILE);
    assert!(
        (placement.pos - Vec3::new(0.3, 0.0, 0.0)).length() < 1e-3,
        "landed at {}",
        placement.pos
    );
    let portal = placement.portal(PortalPairId(1), PortalEnd::A, &carriers);
    let local = Vec3::from(portal.pos);
    assert!(
        (local - Vec3::new(0.3 - 4.0 / 60.0, 0.0, 0.0)).length() < 1e-3,
        "carrier-local pos was {local}"
    );
    let frame = PortalFrame::from_portal(&portal, &carriers);
    assert!((frame.center - placement.pos).length() < 1e-4);
}

#[test]
fn a_shot_that_does_not_fit_where_it_hits_nudges_onto_the_carrier() {
    let layout = tile_wall_layout(false);
    let (world, carriers) = tile_world(&layout, 1);

    let placement = compute_portal_placement(
        Vec3::new(0.3, 3.0, 0.5),
        Vec3::NEG_Y,
        0.0,
        40.0,
        &world,
        &layout,
        &carriers,
        Default::default(),
        &[],
    )
    .expect("a shot near the tile's edge fizzled");

    assert_eq!(placement.carrier, TILE);
    let offset = placement.pos - tile_center(&carriers);
    assert!(
        offset.x.abs() + PORTAL_HALF_WIDTH * PORTAL_RIM_SCALE <= 1.5 + 1e-3
            && offset.z.abs() + PORTAL_HALF_HEIGHT * PORTAL_RIM_SCALE <= 1.5 + 1e-3,
        "aperture hangs past the tile at {}",
        placement.pos
    );
}

#[test]
fn a_shot_over_the_tile_edge_lands_fully_on_one_surface() {
    let layout = tile_wall_layout(true);
    let (world, carriers) = tile_world(&layout, 1);
    let tile_edge = tile_center(&carriers).x + 1.5;

    let over_edge = compute_portal_placement(
        Vec3::new(1.3, 3.0, 0.0),
        Vec3::NEG_Y,
        0.0,
        40.0,
        &world,
        &layout,
        &carriers,
        Default::default(),
        &[],
    )
    .expect("a shot over the tile's edge fizzled");
    assert_eq!(over_edge.carrier, TILE);
    assert!(
        over_edge.pos.x + PORTAL_HALF_WIDTH * PORTAL_RIM_SCALE <= tile_edge + 1e-3,
        "aperture hangs past the tile's edge at {}",
        over_edge.pos
    );

    let on_floor = compute_portal_placement(
        Vec3::new(3.0, 3.0, 0.0),
        Vec3::NEG_Y,
        0.0,
        40.0,
        &world,
        &layout,
        &carriers,
        Default::default(),
        &[],
    )
    .expect("a shot at the floor beside the tile fizzled");
    assert_eq!(on_floor.carrier, CarrierId::WORLD);
}

#[test]
fn placement_portal_is_carrier_local() {
    let layout = tile_wall_layout(false);
    let (_, carriers) = tile_world(&layout, 1);
    let placement = PortalPlacement {
        pos: Vec3::new(1.0, 2.0, 3.0),
        normal: Vec3::NEG_X,
        yaw: 1.25,
        carrier: TILE,
    };

    let portal = placement.portal(PortalPairId(7), PortalEnd::B, &carriers);
    assert_eq!(Vec3::from(portal.pos), placement.pos - tile_center(&carriers));
    assert_eq!(Vec3::new(portal.nx, portal.ny, portal.nz), placement.normal);
    assert_eq!(
        (portal.pair, portal.end, portal.yaw, portal.carrier),
        (PortalPairId(7), PortalEnd::B, 1.25, TILE)
    );

    let free = PortalPlacement {
        carrier: CarrierId::WORLD,
        ..placement
    }
    .portal(PortalPairId(7), PortalEnd::B, &carriers);
    assert_eq!(Vec3::from(free.pos), placement.pos);
}

#[test]
fn a_carried_portal_follows_its_carrier_after_a_refresh() {
    let layout = tile_wall_layout(false);
    let (mut world, mut carriers) = tile_world(&layout, 1);
    let mut set = PortalSet::rebuild(&[carried_portal(0.0), wall_portal()], &world, &carriers);
    assert!(set.has_carried());
    let before = frames(&set).0.center;
    assert!((before - tile_center(&carriers)).length() < 1e-5);

    advance_tile(&mut world, &mut carriers, &mut set, 2);

    let after = frames(&set).0.center;
    assert!(
        (after - before - Vec3::new(4.0 / 60.0, 0.0, 0.0)).length() < 1e-5,
        "frame moved from {before} to {after}"
    );
    assert!(!pair(Vec3::ZERO, Vec3::Y, Vec3::new(9.0, 0.0, 0.0), Vec3::Y).has_carried());
}

#[test]
fn a_body_dropped_into_a_sliding_aperture_exits_the_wall_portal() {
    let layout = tile_wall_layout(false);
    let (mut world, mut carriers) = tile_world(&layout, 1);
    let mut set = PortalSet::rebuild(&[carried_portal(0.0), wall_portal()], &world, &carriers);
    let physics = player_physics();
    let start = Position {
        x: tile_center(&carriers).x,
        y: 0.2,
        z: 0.0,
    };

    let (hop, _, _) = run_ticks(&mut world, &mut carriers, &mut set, physics, start, 2, 30);

    let (tick, hop) = hop.expect("the body never crossed the sliding aperture");
    // Carried along until it crossed: the entry is where the tile was then.
    let tile_at_crossing = carrier_offset_at(&layout.carriers[0], tick);
    assert!(
        (hop.entry.center - tile_at_crossing).length() < 1e-3,
        "entry {} vs tile {}",
        hop.entry.center,
        tile_at_crossing
    );
    assert!(
        hop.origin.z > -10.0 && hop.origin.z < -8.5 && hop.origin.x.abs() < 1.5,
        "did not exit at the wall portal: {}",
        hop.origin
    );
}

#[test]
fn a_body_dropped_where_the_aperture_was_lands_on_the_tile() {
    let layout = tile_wall_layout(false);
    let (mut world, mut carriers) = tile_world(&layout, 15);
    let mut set = PortalSet::rebuild(&[carried_portal(0.0), wall_portal()], &world, &carriers);
    let physics = player_physics();
    let start = Position { x: 0.0, y: 0.2, z: 0.0 };

    let (hop, pos, support) = run_ticks(&mut world, &mut carriers, &mut set, physics, start, 16, 30);

    assert!(hop.is_none(), "hopped at tick {:?}", hop.map(|(tick, _)| tick));
    assert_eq!(support, CharacterSupport::Ground);
    assert!(pos.y.abs() < 0.05, "did not land on the tile: {pos:?}");
    assert!(pos.x > 1.0, "did not ride the tile after landing: {pos:?}");
}

#[test]
fn a_rider_beside_the_aperture_rides_a_full_cycle_without_a_hop() {
    let layout = tile_wall_layout(false);
    let (mut world, mut carriers) = tile_world(&layout, 1);
    let mut set = PortalSet::rebuild(
        &[carried_portal(std::f32::consts::FRAC_PI_2), wall_portal()],
        &world,
        &carriers,
    );
    let physics = player_physics();
    let start = Position {
        x: tile_center(&carriers).x,
        y: 0.0,
        z: 1.1,
    };

    let (hop, pos, support) = run_ticks(&mut world, &mut carriers, &mut set, physics, start, 2, 120);

    assert!(hop.is_none(), "hopped at tick {:?}", hop.map(|(tick, _)| tick));
    assert_eq!(support, CharacterSupport::Ground);
    assert!((pos.z - 1.1).abs() < 0.05, "drifted across the tile: {pos:?}");
    assert!(
        (pos.x - tile_center(&carriers).x).abs() < 0.05,
        "lost the tile: {pos:?} vs {}",
        tile_center(&carriers)
    );
}

#[test]
fn a_rising_plane_catches_a_crossing_the_stale_test_would_miss() {
    let mut layout = tile_wall_layout(false);
    layout.carriers[0].to = Position {
        x: 0.0,
        y: LEVEL_HEIGHT,
        z: 0.0,
    };
    layout.carriers[0].levels = 1;
    let (world, carriers) = tile_world(&layout, 1);
    let rise = LEVEL_HEIGHT / 60.0;
    let carried = PortalSet::rebuild(&[carried_portal(0.0), wall_portal()], &world, &carriers);
    let physics = player_physics();
    let half_y = physics.collider.top_y_offset() / 2.0;
    // The body's center sat 0.05 above the plane last tick and moved down
    // 0.1 while the plane rose `rise`.
    let previous_top = tile_center(&carriers).y - rise;
    let from = Vec3::new(0.0, previous_top + 0.05 - half_y, 0.0);
    let to = from - Vec3::Y * 0.1;

    assert!(
        carried
            .character_hop(from, to, physics, Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, -3.0, 0.0, CAP)
            .is_some(),
        "the rising plane's crossing was missed"
    );

    // Judged only against where the plane is now, the same motion looks
    // like it started behind it.
    let stale_portal = Portal {
        carrier: CarrierId::WORLD,
        pos: tile_center(&carriers).into(),
        ..carried_portal(0.0)
    };
    let stale = PortalSet::rebuild(&[stale_portal, wall_portal()], &world, &carriers);
    assert!(
        stale
            .character_hop(from, to, physics, Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, -3.0, 0.0, CAP)
            .is_none()
    );
}

#[test]
fn a_static_portal_ignores_a_carrier_floor_passing_behind_it() {
    use crate::test_geometry::FLOOR_THICKNESS;

    // A static floor portal at the origin, and a tile sitting just under
    // the plane, inside the aperture's backing volume, as the set is built.
    let mut layout = tile_wall_layout(false);
    layout.floors.push(Floor {
        x1: -3.0,
        z1: -3.0,
        x2: 3.0,
        z2: 3.0,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
        carrier: CarrierId::WORLD,
    });
    layout.carriers[0].from = Position {
        x: 0.0,
        y: -0.1,
        z: 0.0,
    };
    layout.carriers[0].to = Position {
        x: 6.0,
        y: -0.1,
        z: 0.0,
    };
    let (world, carriers) = tile_world(&layout, 0);
    let set = PortalSet::rebuild(
        &[portal(PortalEnd::A, Vec3::ZERO, Vec3::Y, 0.0), wall_portal()],
        &world,
        &carriers,
    );

    let excluded = set.collision_exclusions(Vec3::ZERO, player_physics());
    assert!(!excluded.is_empty(), "the static floor does not back its own portal");
    assert!(
        excluded.iter().all(|handle| world.carrier_of(*handle).is_world()),
        "the passing tile was taken as backing"
    );
}
