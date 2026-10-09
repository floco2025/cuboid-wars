use crate::actors::test_kinds;
use common::config::ActorLocomotion;

#[test]
fn flying_actor_rejects_ground_only_abilities() {
    let mut actor = test_kinds::kind(test_kinds::BEAM);
    actor.character.locomotion = ActorLocomotion::Flying;
    actor.validate("flyer").expect("valid flying configuration rejected");
    actor.character.can_use_ladders = true;
    assert!(actor.validate("flyer").is_err());
    actor.character.can_use_ladders = false;
    actor.character.immovable = true;
    assert!(actor.validate("flyer").is_err());
}
