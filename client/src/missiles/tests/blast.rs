use super::super::fixtures::{wall, world};
use super::*;
use crate::test_fixtures::gameplay_config;
use common::protocol::{ActorId, MapLayout, PlayerGeneration, PlayerId};

#[test]
fn blast_hits_fall_off_with_distance_shove_outward_and_stop_at_cover() {
    let world = world(&MapLayout {
        walls: vec![wall(-3.0, 2.0, 3.0, 2.0)],
        ..Default::default()
    });
    let physics = gameplay_config().player.physics();
    let victim = |target, x, z| (target, Position { x, y: 0.0, z }, physics);
    let near = HitTarget::Player {
        id: PlayerId(1),
        generation: PlayerGeneration(0),
    };
    let far = HitTarget::Actor(ActorId(2));
    let covered = HitTarget::Actor(ActorId(3));
    let hits = missile_blast_hits(
        Vec3::new(0.0, 1.0, 0.0),
        6.0,
        &world,
        &[],
        [
            victim(near, 1.0, 0.0),
            victim(far, 4.0, 0.0),
            victim(covered, 0.0, 4.0),
            victim(HitTarget::Actor(ActorId(4)), 20.0, 0.0),
        ]
        .into_iter(),
    );
    assert_eq!(hits.len(), 2, "cover and range keep the other two out");
    assert_eq!(hits[0].target, near);
    assert_eq!(hits[1].target, far);
    assert!(hits[0].falloff > hits[1].falloff);
    assert!((hits[0].direction[0] - 1.0).abs() < 1e-5 && hits[0].direction[1].abs() < 1e-5);
}
