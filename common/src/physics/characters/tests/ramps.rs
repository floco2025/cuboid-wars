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
