use super::*;

#[test]
fn global_budget_clamps_and_releases_particles() {
    let mut budget = ExplosionVfxBudget::default();
    assert_eq!(
        budget.reserve_shards(EXPLOSION_SHARD_GLOBAL_MAX_COUNT + 10, EXPLOSION_SHARD_GLOBAL_MAX_COUNT,),
        EXPLOSION_SHARD_GLOBAL_MAX_COUNT
    );
    assert_eq!(budget.reserve_shards(1, EXPLOSION_SHARD_GLOBAL_MAX_COUNT), 0);
    budget.release_shards(20);
    assert_eq!(budget.reserve_shards(30, EXPLOSION_SHARD_GLOBAL_MAX_COUNT), 20);
}
