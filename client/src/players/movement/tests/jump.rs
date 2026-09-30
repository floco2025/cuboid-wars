use super::super::fixtures::*;
use common::physics::{CharacterEnvironment, CharacterStep, LadderMode, step_character_movement};

const TEST_JUMP_SPEED: f32 = 12.0;

// One motor step of a held body pushed by `displacement` alone.
fn shoved_step(world: &CollisionWorld, start: Position, displacement: Vec3) -> CharacterMovementResult {
    step_character_movement(
        CharacterStep {
            start,
            vertical_velocity: 0.0,
            intent_velocity: Vec3::ZERO,
            velocity: Vec3::ZERO,
            displacement,
            delta: 0.1,
        },
        &CharacterEnvironment {
            collision_world: world,
            gravity: TEST_GRAVITY,
            passable_fields: &[],
            physics: player_physics(),
            ladder_mode: LadderMode::Automatic,
            portals: None,
            carriers: &Carriers::default(),
        },
    )
}

#[test]
fn supported_player_can_start_jump() {
    let floor = lower_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position { x: 0.0, y: 0.0, z: 0.0 };

    assert_eq!(
        player_jump(
            CharacterSupport::Ground,
            PlayerMoveIntent::NONE,
            0.0,
            &collision_world,
            player_physics(),
            &test_movement(),
            false,
            &pos,
            &[]
        ),
        Some(PlayerJump::Rise(TEST_JUMP_SPEED))
    );
}

#[test]
fn airborne_player_cannot_start_jump() {
    let floor = lower_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position { x: 0.0, y: 1.0, z: 0.0 };

    assert_eq!(
        player_jump(
            CharacterSupport::Airborne,
            PlayerMoveIntent::NONE,
            0.0,
            &collision_world,
            player_physics(),
            &test_movement(),
            false,
            &pos,
            &[]
        ),
        None
    );
}

#[test]
fn a_bridge_its_key_passes_is_no_floor_to_jump_from() {
    use common::{
        physics::passable_fields,
        protocol::{FieldId, LightBridge},
    };
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
#[test]
fn jump_on_a_ladder_lets_go_with_a_shove_clear_of_it() {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    let movement = test_movement();
    let mut pos = Position {
        x: 0.0,
        y: 2.0,
        z: rail_plane_z() - player_hold_distance(),
    };
    let jump = player_jump(
        CharacterSupport::Ladder,
        PlayerMoveIntent::NONE,
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
    assert_eq!(shove.y, 0.0);
    assert!(shove.z < 0.0, "{shove:?}");
    let mut knockback = KnockbackVelocity(shove);
    while knockback.0 != Vec3::ZERO {
        pos = shoved_step(&world, pos, knockback.step(0.1)).position;
        knockback.decay(0.1, movement.knockback.deceleration);
    }
    assert!(world.ladder_volume_at(&pos).is_none(), "{pos:?}");
}

#[test]
fn forward_jump_on_a_ladder_crosses_through_the_rungs() {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    let movement = test_movement();
    let mut pos = Position {
        x: 0.0,
        y: 2.0,
        z: rail_plane_z() - player_hold_distance(),
    };
    let forward = PlayerMoveIntent::moving(0.0);
    let jump = player_jump(
        CharacterSupport::Ladder,
        forward,
        4.0,
        &world,
        player_physics(),
        &movement,
        false,
        &pos,
        &[],
    );
    let Some(PlayerJump::Release(shove)) = jump else {
        panic!("a held climber's forward jump did not let go: {jump:?}");
    };
    assert!(shove.z > 0.0, "{shove:?}");
    let mut knockback = KnockbackVelocity(shove);
    while knockback.0 != Vec3::ZERO {
        pos = shoved_step(&world, pos, knockback.step(0.1)).position;
        knockback.decay(0.1, movement.knockback.deceleration);
    }
    assert!(
        pos.z > rail_plane_z() && world.ladder_volume_at(&pos).is_none(),
        "{pos:?}"
    );
}

#[test]
fn jump_refused_airborne_inside_ladder_volume() {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    let pos = Position {
        x: 0.0,
        y: 2.0,
        z: rail_plane_z() - player_hold_distance(),
    };
    assert_eq!(
        player_jump(
            CharacterSupport::Airborne,
            PlayerMoveIntent::NONE,
            0.0,
            &world,
            player_physics(),
            &test_movement(),
            false,
            &pos,
            &[]
        ),
        None
    );
}

#[test]
fn jump_refused_airborne_outside_ladder() {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    let pos = Position {
        x: 0.0,
        y: 2.0,
        z: -3.0,
    };
    assert_eq!(
        player_jump(
            CharacterSupport::Airborne,
            PlayerMoveIntent::NONE,
            0.0,
            &world,
            player_physics(),
            &test_movement(),
            false,
            &pos,
            &[]
        ),
        None
    );
}

#[test]
fn jump_refused_airborne_behind_ladder() {
    let world = ladder_collision_world(&[], &[test_ladder()]);
    let pos = Position { x: 0.0, y: 2.0, z: 0.4 };
    assert_eq!(
        player_jump(
            CharacterSupport::Airborne,
            PlayerMoveIntent::NONE,
            0.0,
            &world,
            player_physics(),
            &test_movement(),
            false,
            &pos,
            &[]
        ),
        None
    );
}
