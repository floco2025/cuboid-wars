use super::*;

fn info(shooter: u32, born_tick: u32) -> MissileInfo {
    MissileInfo {
        entity: Entity::PLACEHOLDER,
        shooter: PlayerId(shooter),
        born_tick,
        impact_pending: false,
    }
}

#[test]
fn a_snapshot_removes_only_others_finished_missiles_launched_at_or_before_its_tick() {
    let mut missiles = MissileMap::default();
    missiles.insert(MissileId(1), info(2, 10));
    missiles.insert(MissileId(2), info(2, 21));
    missiles.insert(MissileId(3), info(1, 10));
    missiles.insert(MissileId(4), info(2, 10));
    let mut ending = info(2, 10);
    ending.impact_pending = true;
    missiles.insert(MissileId(5), ending);
    let stale = stale_missile_ids(&missiles, PlayerId(1), 20, &HashSet::from([MissileId(4)]));
    assert_eq!(stale, [MissileId(1)]);
    let mut later = stale_missile_ids(&missiles, PlayerId(1), 21, &HashSet::new());
    later.sort_unstable_by_key(|id| id.0);
    assert_eq!(later, [MissileId(1), MissileId(2), MissileId(4)]);
}
