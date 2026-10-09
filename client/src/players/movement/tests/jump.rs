use super::super::fixtures::*;
use common::{
    physics::{CharacterEnvironment, CharacterStep, LadderMode, passable_fields, step_character_movement},
    protocol::{FieldId, LightBridge},
};

const TEST_JUMP_SPEED: f32 = 12.0;

#[test]
fn a_bridge_its_key_passes_is_no_floor_to_jump_from() {
    let collision_world = CollisionWorld::from_map_layout(&MapLayout {
        light_bridges: vec![LightBridge {
            x1: -2.0,
            z1: -2.0,
            x2: 2.0,
            z2: 2.0,
            y: 0.0,
            thickness: BRIDGE_THICKNESS,
            level: 0,
            field: FieldId(3),
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    });
    let pos = Position { x: 0.0, y: 0.0, z: 0.0 };
    let jump = |held_keys: &[FieldId]| {
        let passable = passable_fields(held_keys, &[]);
        player_jump(
            CharacterSupport::Ground,
            PlayerMoveIntent::NONE,
            0.0,
            &collision_world,
            player_physics(),
            &test_movement(),
            false,
            &pos,
            &passable,
        )
    };

    assert_eq!(jump(&[]), Some(PlayerJump::Rise(TEST_JUMP_SPEED)));
    assert_eq!(jump(&[FieldId(1)]), Some(PlayerJump::Rise(TEST_JUMP_SPEED)));
    assert_eq!(jump(&[FieldId(3)]), None);
}

// A held climber's jump with `intent`: the shove it lets go with, and where
// the body is once the shove has decayed, which is clear of the ladder.
fn let_go_of_the_ladder(intent: PlayerMoveIntent) -> (Vec3, Position) {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    let movement = test_movement();
    let mut pos = Position {
        x: 0.0,
        y: 2.0,
        z: rail_plane_z() - player_hold_distance(),
    };
    let jump = player_jump(
        CharacterSupport::Ladder,
        intent,
        4.0,
        &world,
        player_physics(),
        &movement,
        false,
        &pos,
        &[],
    );
    let Some(PlayerJump::Release(shove)) = jump else {
        panic!("a held climber's jump did not let go: {jump:?}");
    };
    let environment = CharacterEnvironment {
        collision_world: &world,
        gravity: TEST_GRAVITY,
        passable_fields: &[],
        physics: player_physics(),
        ladder_mode: LadderMode::Automatic,
        portals: None,
        carriers: &Carriers::default(),
    };
    let mut knockback = KnockbackVelocity(shove);
    while knockback.0 != Vec3::ZERO {
        let step = CharacterStep {
            start: pos,
            vertical_velocity: 0.0,
            intent_velocity: Vec3::ZERO,
            velocity: Vec3::ZERO,
            displacement: knockback.step(0.1),
            delta: 0.1,
        };
        pos = step_character_movement(step, &environment).position;
        knockback.decay(0.1, movement.knockback.deceleration);
    }
    assert!(world.ladder_volume_at(&pos).is_none(), "{pos:?}");
    (shove, pos)
}

#[test]
fn jump_on_a_ladder_lets_go_with_a_shove_clear_of_it() {
    let (shove, _) = let_go_of_the_ladder(PlayerMoveIntent::NONE);
    assert_eq!(shove.y, 0.0);
    assert!(shove.z < 0.0, "{shove:?}");
}

#[test]
fn forward_jump_on_a_ladder_crosses_through_the_rungs() {
    let (shove, pos) = let_go_of_the_ladder(PlayerMoveIntent::moving(0.0));
    assert!(shove.z > 0.0, "{shove:?}");
    assert!(pos.z > rail_plane_z(), "{pos:?}");
}

#[test]
fn an_airborne_body_gets_no_jump_inside_or_around_a_ladders_volume() {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    for z in [rail_plane_z() - player_hold_distance(), -3.0, 0.4] {
        let pos = Position { x: 0.0, y: 2.0, z };
        let jump = player_jump(
            CharacterSupport::Airborne,
            PlayerMoveIntent::NONE,
            0.0,
            &world,
            player_physics(),
            &test_movement(),
            false,
            &pos,
            &[],
        );
        assert_eq!(jump, None, "{pos:?}");
    }
}
