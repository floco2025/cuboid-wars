use super::*;
use crate::test_fixtures;
use bevy::ecs::system::SystemState;
use common::{
    constants::TICK_SECS,
    protocol::{BarrierKindTable, MapLayout},
};
use std::f32::consts::FRAC_PI_2;

#[test]
fn remote_bodies_stay_at_reported_positions_while_the_owner_simulates() {
    let gameplay = test_fixtures::gameplay_config();
    let settings = test_fixtures::map_settings();
    let collision = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());
    for is_local in [false, true] {
        let mut world = World::new();
        let position = Position {
            x: 0.0,
            y: 10.0,
            z: 0.0,
        };
        let entity = world
            .spawn((
                PlayerMarker,
                PlayerId(1),
                position,
                PlayerMoveIntent::Walking { direction: FRAC_PI_2 },
                CharacterVerticalVelocity(-3.0),
                AirborneMomentum(Vec3::X),
                KnockbackVelocity(Vec3::X),
                PlayerAnimationMotion::default(),
            ))
            .id();
        if is_local {
            world.entity_mut(entity).insert(LocalPlayerMarker);
        }
        let mut state = SystemState::<(Commands, PlayerMovementQuery)>::new(&mut world);
        let (mut commands, mut query) = state.get_mut(&mut world).expect("movement query invalid");
        let mut plans = Vec::new();
        plan_player_moves(
            &mut commands,
            TICK_SECS,
            &collision,
            &settings,
            &gameplay,
            &PlayerMap::default(),
            &PlateState::default(),
            &PortalSet::default(),
            &Carriers::default(),
            false,
            &mut query,
            &mut plans,
        );
        state.apply(&mut world);
        let plan = plans.first().expect("movement plan missing");
        if is_local {
            assert!(plan.target.x > position.x);
            assert!(plan.target.y < position.y);
        } else {
            assert_eq!(plan.target, position);
            assert_eq!(plan.target_vertical_velocity, -3.0);
            assert_eq!(
                world
                    .get::<PlayerAnimationMotion>(entity)
                    .expect("animation missing")
                    .velocity,
                Vec3::ZERO
            );
        }
    }
}

#[test]
fn a_dead_local_player_gets_no_plan() {
    let gameplay = test_fixtures::gameplay_config();
    let settings = test_fixtures::map_settings();
    let collision = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());
    let mut world = World::new();
    let position = Position {
        x: 0.0,
        y: 10.0,
        z: 0.0,
    };
    let entity = world
        .spawn((
            PlayerMarker,
            LocalPlayerMarker,
            PlayerId(1),
            position,
            PlayerMoveIntent::Walking { direction: FRAC_PI_2 },
            CharacterVerticalVelocity(-3.0),
            AirborneMomentum(Vec3::X),
            KnockbackVelocity(Vec3::X),
            PlayerAnimationMotion::default(),
        ))
        .id();
    let mut state = SystemState::<(Commands, PlayerMovementQuery)>::new(&mut world);
    let (mut commands, mut query) = state.get_mut(&mut world).expect("movement query invalid");
    let mut plans = Vec::new();
    plan_player_moves(
        &mut commands,
        TICK_SECS,
        &collision,
        &settings,
        &gameplay,
        &PlayerMap::default(),
        &PlateState::default(),
        &PortalSet::default(),
        &Carriers::default(),
        true,
        &mut query,
        &mut plans,
    );
    state.apply(&mut world);
    assert!(plans.is_empty());
    assert_eq!(*world.get::<Position>(entity).expect("position missing"), position);
}
