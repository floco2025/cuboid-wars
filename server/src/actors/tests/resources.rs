use super::*;
use crate::actors::test_kinds;

#[test]
fn actor_map_records_vacated_zones() {
    let mut actors = ActorMap::default();
    let id = ActorId(4);
    let entity = Entity::from_bits(12);
    actors.insert(
        id,
        ActorInfo::new(entity, 3, test_kinds::BEAM.to_owned(), CarrierId::WORLD),
    );

    assert!(!actors.has_vacated_spawn_zones());

    actors.remove(&id);

    assert_eq!(actors.drain_vacated_spawn_zones().collect::<Vec<_>>(), vec![3]);
}
