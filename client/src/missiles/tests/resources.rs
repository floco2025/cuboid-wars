use super::*;

#[test]
fn detonated_missiles_stay_retired_until_snapshots_have_passed_their_death() {
    let mut missiles = MissileMap::default();
    assert!(missiles.retire(MissileId(1), u32::MAX));
    assert!(!missiles.retire(MissileId(1), u32::MAX));
    for tick in [u32::MAX - 1, u32::MAX] {
        missiles.discard_retired_before(tick);
        assert!(missiles.is_retired(&MissileId(1)));
    }
    missiles.discard_retired_before(0);
    assert!(!missiles.is_retired(&MissileId(1)));
}

#[test]
fn a_flight_ended_here_consumes_its_own_detonation_cue_once() {
    let mut missiles = MissileMap::default();
    missiles.insert(
        MissileId(4),
        MissileInfo {
            entity: Entity::PLACEHOLDER,
            shooter: PlayerId(1),
            born_tick: 3,
            impact_pending: false,
        },
    );
    assert!(missiles.remove(&MissileId(4)).is_some());
    assert!(!missiles.retire(MissileId(4), 9));
    assert!(missiles.is_retired(&MissileId(4)));
    assert!(!missiles.retire(MissileId(4), 9));
}
