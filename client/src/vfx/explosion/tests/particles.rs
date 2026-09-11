use super::*;

#[test]
fn global_budget_clamps_and_releases_particles() {
    let mut budget = ExplosionVfxBudget::default();
    assert_eq!(budget.reserve_shards(12, 8), 8);
    assert_eq!(budget.reserve_shards(1, 8), 0);
    budget.release_shards(3);
    assert_eq!(budget.reserve_shards(5, 8), 3);
}
