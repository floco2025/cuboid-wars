use super::*;
use crate::test_fixtures::gameplay_config;
use common::{
    constants::TICK_SECS,
    map::Carriers,
    physics::{CharacterVerticalVelocity, CollisionWorld, KnockbackVelocity},
    protocol::{CarrierId, MapLayout, Portal, PortalEnd, PortalPairId},
};

fn pair(a_pos: Vec3, a_normal: Vec3, b_pos: Vec3, b_normal: Vec3) -> PortalSet {
    let portal = |end, pos: Vec3, normal: Vec3| Portal {
        pair: PortalPairId(1),
        end,
        pos: pos.into(),
        nx: normal.x,
        ny: normal.y,
        nz: normal.z,
        yaw: 0.0,
        carrier: CarrierId::WORLD,
    };
    let gameplay = gameplay_config();
    PortalSet::rebuild(
        &[
            portal(PortalEnd::A, a_pos, a_normal),
            portal(PortalEnd::B, b_pos, b_normal),
        ],
        &CollisionWorld::from_map_layout(&MapLayout::default()),
        &Carriers::default(),
        gameplay.portals.size,
    )
}

#[test]
fn falling_into_floor_portal_exits_ramp_at_its_normal_angle_in_the_shorter_hull() {
    let ramp_normal = Vec3::new(0.0, 0.6, 0.8);
    let set = pair(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::Y,
        Vec3::new(10.0, 2.0, 10.0),
        ramp_normal,
    );
    let gameplay = gameplay_config();
    let movement = crate::test_fixtures::map_settings().movement;
    let hop = player_hop(
        &set,
        Vec3::new(0.0, -0.85, 0.0),
        Vec3::new(0.0, -0.95, 0.0),
        &gameplay,
        &movement,
        PlayerHopBody {
            stance: Default::default(),
            knockback: &KnockbackVelocity::default(),
            horizontal_velocity: &HorizontalVelocity::default(),
            vertical_velocity: -10.0,
            carried: Vec3::ZERO,
            yaw: 0.0,
        },
        TICK_SECS,
    )
    .expect("floor-to-ramp portal crossing missing");
    let crossing = &hop.crossing;
    let exit_velocity = crossing.horizontal_velocity + crossing.knockback + Vec3::Y * crossing.vertical_velocity;

    assert!((exit_velocity - ramp_normal * 10.0).length() < 1e-4);
    assert!(crossing.knockback.length() < 1e-4);
    assert!(crossing.horizontal_velocity.z > 1.0);
    assert!(hop.force_crouch, "an angled fling uses the shorter exit hull");
    // The crouched hull keeps the centre the standing one had.
    let standing = gameplay.player.physics().movement_collider.height;
    let crouched = PlayerStance { crouched: true }
        .physics(&gameplay.player)
        .movement_collider
        .height;
    let mut stance = PlayerStance::default();
    let mut position = Position::default();
    let mut intent = PlayerMoveIntent::NONE;
    hop.apply(
        &mut position,
        &mut FaceYaw(0.0),
        &mut CharacterVerticalVelocity(0.0),
        &mut intent,
        &mut stance,
        &mut KnockbackVelocity::default(),
        &mut HorizontalVelocity::default(),
    );
    assert!(stance.crouched);
    assert_eq!(position, Position::from(crossing.origin));
    let raised = set
        .character_hop(
            Vec3::new(0.0, -0.85, 0.0),
            Vec3::new(0.0, -0.95, 0.0),
            gameplay.player.physics(),
            common::physics::CharacterHopBody {
                knockback: Vec3::ZERO,
                horizontal_velocity: Vec3::ZERO,
                vertical_velocity: -10.0,
                carried: Vec3::ZERO,
                yaw: 0.0,
            },
            0.0,
            TICK_SECS,
        )
        .expect("the shared hop crosses too")
        .origin;
    assert!(((crossing.origin.y - raised.y) - (standing - crouched) * 0.5).abs() < 1e-5);
}
