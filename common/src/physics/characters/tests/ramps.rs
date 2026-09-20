use super::*;

#[test]
fn player_walking_off_ramp_side_is_not_blocked_by_ramp_side() {
    let ramp = test_ramp();
    let collision_world = collision_world(&[], &[ramp]);
    let pos = Position {
        x: 2.0,
        y: ramp.surface_at(2.0, 4.0),
        z: 4.0,
    };
    let motion = 0.0;

    let step = step_in(
        &collision_world,
        character_step_toward(pos, motion, -1.0, pos.z, 0.1),
        LadderMode::Automatic,
    );

    assert!(!step.blocked, "{step:?}");
    assert!(step.position.x < pos.x);
}

#[test]
fn a_solid_ramp_fills_the_space_under_its_high_end_and_a_plank_leaves_it_open() {
    for (shape, blocked) in [(RampShape::Solid, true), (RampShape::Plank, false)] {
        let ramp = Ramp { shape, ..test_ramp() };
        let collision_world = collision_world(&[], &[ramp]);
        let pos = Position {
            x: -1.0,
            y: 0.0,
            z: 7.0,
        };

        let step = step_in(
            &collision_world,
            character_step_toward(pos, 0.0, 1.0, pos.z, 0.1),
            LadderMode::Automatic,
        );

        assert_eq!(step.blocked, blocked, "{shape:?}: {step:?}");
        assert_eq!(step.position.x < 0.0, blocked, "{shape:?}: {step:?}");
    }
}

#[test]
fn lower_floor_player_can_enter_wedge_low_end() {
    let ramp = test_ramp();
    let collision_world = collision_world(&[], &[ramp]);
    let pos = Position {
        x: 2.0,
        y: 0.0,
        z: -0.25,
    };
    let motion = 0.0;

    let step = step_in(
        &collision_world,
        character_step_toward(pos, motion, pos.x, 0.25, 0.1),
        LadderMode::Automatic,
    );

    assert!(!step.blocked, "{step:?}");
    assert!(step.position.z > pos.z);
}

#[test]
fn upper_floor_player_can_enter_wedge_high_end() {
    let ramp = test_ramp();
    let collision_world = collision_world(&[], &[ramp]);
    let pos = Position {
        x: 2.0,
        y: LEVEL_HEIGHT,
        z: 8.25,
    };
    let motion = 0.0;

    let step = step_in(
        &collision_world,
        character_step_toward(pos, motion, pos.x, 7.75, 0.1),
        LadderMode::Automatic,
    );

    assert!(!step.blocked, "{step:?}");
    assert!(step.position.z < pos.z);
}

#[test]
fn capsule_cannot_step_sideways_onto_a_floor_above_step_height() {
    let ramp = test_ramp();
    let floor = upper_floor_west_of_ramp();
    let collision_world = collision_world(&[floor], &[ramp]);
    let y = ramp.surface_at(2.0, 7.0);
    let pos = Position { x: 2.0, y, z: 7.0 };
    let motion = 0.0;

    let step = step_in(
        &collision_world,
        character_step_toward(pos, motion, -1.0, pos.z, 0.1),
        LadderMode::Automatic,
    );

    assert!(step.blocked, "{step:?}");
    assert!(step.position.x > floor.x2, "climbed a high ledge: {step:?}");
    assert!(step.position.x < pos.x);
}

#[test]
fn capsule_keeps_walking_from_level_ground_onto_a_shallow_terrain_slope() {
    let layout = MapLayout {
        grounds: Some(crate::map::Grounds::new(
            [(-3.0, 3.0, -3.0, 3.0)],
            0.0,
            crate::map::GroundsSettings { level: 0 },
        )),
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout);
    let mut physics = player_physics();
    physics.movement_collider = crate::config::MovementColliderConfig {
        diameter: 0.8,
        height: 1.8,
    };
    let carriers = Carriers::default();
    let env = test_environment(&world, &carriers, physics, LadderMode::Disabled);
    let mut pos = Position {
        x: 7.148134,
        y: 0.0000009706552,
        z: 1.3129995,
    };
    let mut vertical_velocity = 0.0;
    for _ in 0..120 {
        let result = step_character_movement(
            CharacterStep {
                start: pos,
                vertical_velocity,
                control_velocity: Vec3::new(1.6342_f32.sin() * 8.0, 0.0, 1.6342_f32.cos() * 8.0),
                external_displacement: Vec3::ZERO,
                delta: 1.0 / 30.0,
            },
            &env,
        );
        pos = result.position;
        vertical_velocity = result.vertical_velocity;
        if pos.x > 18.0 {
            return;
        }
    }
    panic!("stopped on a motor-walkable terrain slope: {pos:?}");
}
