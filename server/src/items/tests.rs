use crate::{
    constants::ITEM_TARGET_ACTIVE,
    resources::{CellGrid, EdgeGrid, LevelGrid, MapConfig},
};

use super::spawn_cells::{ItemSpawnCell, eligible_item_spawn_cells, target_active_power_ups};

fn map_config(levels: Vec<LevelGrid>) -> MapConfig {
    MapConfig {
        levels,
        actor_spawn_zones: Vec::new(),
        player_spawn_zones: Vec::new(),
        cookie_spawn_zones: Vec::new(),
    }
}

fn level_grid(cells: CellGrid) -> LevelGrid {
    LevelGrid {
        cells,
        edges: EdgeGrid::new(1, 1),
    }
}

#[test]
fn item_spawn_cells_include_all_floor_levels_and_skip_ramps() {
    let mut lower = CellGrid::new(1, 1);
    lower.rows[0][0].has_floor = true;
    let mut upper = CellGrid::new(1, 1);
    upper.rows[0][0].has_floor = true;
    upper.rows[0][0].has_ramp = true;
    let config = map_config(vec![level_grid(lower), level_grid(upper)]);

    let cells = eligible_item_spawn_cells(&config);

    assert_eq!(
        cells,
        vec![ItemSpawnCell {
            level: 0,
            col: 0,
            row: 0
        }]
    );
}

#[test]
fn power_up_target_uses_constant_capped_by_eligible_cells() {
    // Empty / undersized maps degrade gracefully; once there's enough room
    // the count is just `ITEM_TARGET_ACTIVE`.
    assert_eq!(target_active_power_ups(0), 0);
    assert_eq!(target_active_power_ups(1), 1);
    assert_eq!(target_active_power_ups(ITEM_TARGET_ACTIVE - 1), ITEM_TARGET_ACTIVE - 1);
    assert_eq!(target_active_power_ups(ITEM_TARGET_ACTIVE), ITEM_TARGET_ACTIVE);
    assert_eq!(target_active_power_ups(ITEM_TARGET_ACTIVE + 1000), ITEM_TARGET_ACTIVE);
}
