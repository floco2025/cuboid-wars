use super::{super::traversal::traverse_yaw, *};
use crate::{
    config::gameplay::load_test_gameplay,
    constants::CHARACTER_CONTACT_OFFSET,
    math::angle_delta_radians,
    physics::{AirborneMomentum, CharacterVerticalVelocity, KnockbackVelocity},
    protocol::{FaceYaw, PlayerMoveIntent},
};

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
            CharacterHopBody {
                control_velocity: control,
                knockback: Vec3::ZERO,
                airborne_momentum: Vec3::ZERO,
                vertical_velocity: 0.0,
                yaw: PI,
            },
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
            CharacterHopBody {
                control_velocity: next_control,
                knockback: hop.knockback,
                airborne_momentum: hop.airborne_momentum,
                vertical_velocity: hop.vertical_velocity,
                yaw: hop.yaw,
            },
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
fn falling_into_floor_portal_carries_out_of_wall_as_airborne_momentum() {
    let set = pair(Vec3::new(0.0, 0.0, 0.0), Vec3::Y, Vec3::new(10.0, 2.0, 0.0), Vec3::X);
    let hop = set
        .character_hop(
            Vec3::new(0.0, -0.85, 0.0),
            Vec3::new(0.0, -0.95, 0.0),
            player_physics(),
            CharacterHopBody {
                control_velocity: Vec3::ZERO,
                knockback: Vec3::ZERO,
                airborne_momentum: Vec3::ZERO,
                vertical_velocity: -10.0,
                yaw: 0.0,
            },
            CAP,
        )
        .expect("fall through a floor portal did not trigger");
    assert!(hop.vertical_velocity.abs() < 1e-4);
    assert!(hop.knockback.length() < 1e-4);
    assert!((hop.airborne_momentum - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-4);
}

#[test]
fn walking_into_wall_portal_exits_floor_portal_upward() {
    let set = pair(Vec3::new(0.0, 0.9, 0.0), Vec3::Z, Vec3::new(10.0, 0.0, 10.0), Vec3::Y);
    let hop = set
        .character_hop(
            Vec3::new(0.0, 0.0, 0.1),
            Vec3::new(0.0, 0.0, -0.1),
            player_physics(),
            CharacterHopBody {
                control_velocity: Vec3::new(0.0, 0.0, -6.0),
                knockback: Vec3::ZERO,
                airborne_momentum: Vec3::ZERO,
                vertical_velocity: 0.0,
                yaw: PI,
            },
            CAP,
        )
        .expect("walk through a wall portal did not trigger");
    // Control maps into the vertical write but not either momentum carry.
    assert!((hop.vertical_velocity - 6.0).abs() < 1e-4);
    assert!(hop.knockback.length() < 1e-4);
    assert!(hop.airborne_momentum.length() < 1e-4);
    // Emerges half-in: the crossing penetration is carried through.
    assert!((hop.origin.y - (0.1 - 0.9 - CHARACTER_CONTACT_OFFSET)).abs() < 1e-4);
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
    let gameplay = load_test_gameplay().expect("test gameplay config rejected");
    let movement = map_movement();
    let hop = set
        .player_hop(
            Vec3::new(0.0, -0.85, 0.0),
            Vec3::new(0.0, -0.95, 0.0),
            &gameplay,
            &movement,
            PlayerHopBody {
                move_intent: PlayerMoveIntent::Idle,
                has_speed: false,
                stunned: false,
                knockback: &KnockbackVelocity::default(),
                airborne_momentum: &AirborneMomentum::default(),
                vertical_velocity: -10.0,
                yaw: 0.0,
            },
        )
        .expect("floor-to-ramp portal crossing missing");
    let exit_velocity = hop.airborne_momentum + hop.knockback + Vec3::Y * hop.vertical_velocity;

    assert!((exit_velocity - ramp_normal * 10.0).length() < 1e-4);
    assert!(hop.knockback.length() < 1e-4);
    assert!(hop.airborne_momentum.z > 1.0);
}

#[test]
fn crossing_the_plane_triggers_and_carries_penetration() {
    let set = pair(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
    let hop = set
        .character_hop(
            Vec3::new(0.0, 0.7, 0.15),
            Vec3::new(0.0, 0.7, -0.05),
            player_physics(),
            CharacterHopBody {
                control_velocity: Vec3::new(0.0, 0.0, -6.0),
                knockback: Vec3::ZERO,
                airborne_momentum: Vec3::ZERO,
                vertical_velocity: 0.0,
                yaw: PI,
            },
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
        CharacterHopBody {
            control_velocity: Vec3::new(0.0, 0.0, -6.0),
            knockback: Vec3::ZERO,
            airborne_momentum: Vec3::ZERO,
            vertical_velocity: 0.0,
            yaw: PI,
        },
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
        CharacterHopBody {
            control_velocity: Vec3::new(0.0, 0.0, 1.0),
            knockback: Vec3::ZERO,
            airborne_momentum: Vec3::ZERO,
            vertical_velocity: 0.0,
            yaw: 0.0,
        },
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
        CharacterHopBody {
            control_velocity: Vec3::new(0.0, 0.0, -6.0),
            knockback: Vec3::ZERO,
            airborne_momentum: Vec3::ZERO,
            vertical_velocity: 0.0,
            yaw: PI,
        },
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
        CharacterHopBody {
            control_velocity: Vec3::new(0.0, 0.0, -6.0),
            knockback: Vec3::ZERO,
            airborne_momentum: Vec3::ZERO,
            vertical_velocity: 0.0,
            yaw: PI,
        },
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
            CharacterHopBody {
                control_velocity: Vec3::ZERO,
                knockback: Vec3::X * 50.0,
                airborne_momentum: Vec3::ZERO,
                vertical_velocity: -1.0,
                yaw: 0.0,
            },
            CAP,
        )
        .expect("fall through a floor portal did not trigger");
    assert!((hop.knockback.length() - CAP).abs() < 1e-4);
}

#[test]
fn an_external_teleport_is_not_a_crossing() {
    // Sign-crosses the plane, but no tick of real motion jumps this far.
    let set = pair(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
    let hop = set.character_hop(
        Vec3::new(0.0, 50.0, 0.15),
        Vec3::new(0.0, 0.7, -0.05),
        player_physics(),
        CharacterHopBody {
            control_velocity: Vec3::ZERO,
            knockback: Vec3::ZERO,
            airborne_momentum: Vec3::ZERO,
            vertical_velocity: 0.0,
            yaw: PI,
        },
        CAP,
    );
    assert!(hop.is_none());
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
            CharacterHopBody {
                control_velocity: inside_move,
                knockback: Vec3::ZERO,
                airborne_momentum: Vec3::ZERO,
                vertical_velocity: 0.0,
                yaw: PI,
            },
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
            CharacterHopBody {
                control_velocity: outside_move,
                knockback: Vec3::ZERO,
                airborne_momentum: Vec3::ZERO,
                vertical_velocity: 0.0,
                yaw: PI,
            },
            CAP,
        )
        .is_none()
    );
}

#[test]
fn traverse_rotation_turns_vectors_like_traverse_vector() {
    let set = pair(
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.6, 0.8),
        Vec3::new(9.0, 3.0, 1.0),
        Vec3::X,
    );
    let (entry, exit) = frames(&set);
    let rotation = traverse_rotation(entry, exit);
    for v in [Vec3::X, Vec3::Y, Vec3::Z, Vec3::new(1.3, -4.2, 2.9)] {
        assert!((rotation * v - traverse_vector(entry, exit, v)).length() < 1e-4);
    }
}

#[test]
fn traverse_point_carries_an_offset_behind_the_entry_to_the_front_of_the_exit() {
    let set = pair(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
    let (entry, exit) = frames(&set);
    assert!((traverse_point(entry, exit, entry.center) - exit.center).length() < 1e-5);
    let sunk = entry.center - entry.normal * 0.3 + entry.up * 0.2;
    let mapped = traverse_point(entry, exit, sunk);
    assert!(((mapped - exit.center).dot(exit.normal) - 0.3).abs() < 1e-5);
    assert!(((mapped - exit.center).dot(exit.up) - 0.2).abs() < 1e-5);
}

#[test]
fn straddled_gate_is_the_one_whose_plane_the_body_reaches_from_the_front() {
    let set = pair(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, Vec3::new(10.0, 1.0, 10.0), Vec3::X);
    let physics = player_physics();
    let carriers = Carriers::default();
    let gate = set
        .straddled_gate(Vec3::new(0.0, 0.7, 0.15), physics, &carriers, 1.0)
        .expect("a body touching the plane from the front is not straddling it");
    assert_eq!((gate.pair, gate.end), (PortalPairId(1), PortalEnd::A));
    assert!((gate.exit.center - Vec3::new(10.0, 1.0, 10.0)).length() < 1e-5);
    assert!(
        set.straddled_gate(Vec3::new(0.0, 0.7, 2.0), physics, &carriers, 1.0)
            .is_none()
    );
    assert!(
        set.straddled_gate(Vec3::new(0.0, 0.7, -0.15), physics, &carriers, 1.0)
            .is_none()
    );
    assert!(
        set.straddled_gate(Vec3::new(3.0, 0.7, 0.15), physics, &carriers, 1.0)
            .is_none()
    );
    let gate = set
        .straddled_gate(Vec3::new(10.05, 0.1, 10.0), physics, &carriers, 1.0)
        .expect("the body carried past the plane is not straddling the exit");
    assert_eq!(gate.end, PortalEnd::B);
    assert!((gate.exit.center - Vec3::new(0.0, 1.6, 0.0)).length() < 1e-5);
}

#[test]
fn a_carried_gate_is_straddled_where_it_is_drawn() {
    let layout = tile_wall_layout(false);
    let (world, carriers) = tile_world(&layout, 1);
    let set = PortalSet::rebuild(&[carried_portal(0.0), wall_portal()], &world, &carriers);
    let physics = player_physics();
    let current = tile_center(&carriers);
    let travel = current - carriers.pose_between(TILE, 0.0).translation;
    assert!(travel.x > 0.05, "the tile did not slide along x this tick: {travel}");
    // Just inside the aperture's leading edge at this tick's pose, so the
    // pose one tick earlier, drawn at alpha 0, leaves the body outside.
    let origin = Vec3::new(current.x + PORTAL_HALF_WIDTH - 0.02, current.y + 0.01, current.z);
    let gate = set
        .straddled_gate(origin, physics, &carriers, 1.0)
        .expect("the body over the drawn aperture is not straddling it");
    assert!((gate.entry.center - current).length() < 1e-4);
    assert!(set.straddled_gate(origin, physics, &carriers, 0.0).is_none());
}
