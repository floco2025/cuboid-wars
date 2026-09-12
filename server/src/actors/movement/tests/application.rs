use super::*;
use crate::actors::{
    ActorCharacter, ActorCrushed, ActorInfo,
    test_kinds::{self, CONTACT},
};
use bevy::{ecs::system::SystemState, prelude::*};
use common::{
    config::ActorLocomotion,
    physics::{CharacterSupport, CharacterVerticalVelocity},
    protocol::{ActorId, ActorMarker, ActorMoveIntent, CarrierId, FaceYaw, MapLayout, Position},
};

#[test]
fn rejected_flying_step_does_not_apply_its_vertical_velocity_or_crush_result() {
    let mut ecs = World::new();
    let mut character = test_kinds::kind(CONTACT).character;
    character.locomotion = ActorLocomotion::Flying;
    let physics = character.physics();
    let start = Position::default();
    let entity = ecs
        .spawn((
            ActorId(1),
            ActorMarker,
            start,
            CharacterVerticalVelocity(2.0),
            ActorMoveIntent::Idle,
            FaceYaw(0.0),
            CharacterSupport::Airborne,
            ActorCrushed(false),
            ActorCharacter(character),
        ))
        .id();
    let mut actors = ActorMap::default();
    actors.insert(ActorId(1), ActorInfo::new(entity, 0, CONTACT.into(), CarrierId::WORLD));
    let target = Position { y: 2.0, ..start };
    let plans = [
        CharacterMovePlan::from_target(entity, start, target, 20.0, physics, true),
        CharacterMovePlan::stationary(Entity::from_bits(999), target, 0.0, physics),
    ];
    let mut state = SystemState::<ActorMovementQuery>::new(&mut ecs);
    let mut query = state.get_mut(&mut ecs).expect("movement query invalid");
    apply_actor_moves(
        &mut query,
        &actors,
        &plans,
        &CollisionWorld::from_map_layout(&MapLayout::default()),
        &[],
    );
    assert_eq!(*ecs.get::<Position>(entity).expect("actor position missing"), start);
    assert_eq!(
        ecs.get::<CharacterVerticalVelocity>(entity)
            .expect("actor velocity missing")
            .0,
        0.0
    );
    assert!(!ecs.get::<ActorCrushed>(entity).expect("actor crush state missing").0);
}
