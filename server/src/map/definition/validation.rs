use std::collections::BTreeSet;

use anyhow::{Context, Result, anyhow};

use super::{LevelDef, MapDef, MapFile, PlayerSpawnDef, RampDef, SUPPORTED_VERSION};

pub(super) fn validate_file(file: &MapFile) -> Result<()> {
    if file.version != SUPPORTED_VERSION {
        return Err(anyhow!(
            "unsupported map file version {} (expected {})",
            file.version,
            SUPPORTED_VERSION
        ));
    }
    validate_map(&file.map)
}

pub(super) fn validate_map(map_def: &MapDef) -> Result<()> {
    if map_def.grid_cols <= 0 || map_def.grid_rows <= 0 {
        return Err(anyhow!("grid_cols and grid_rows must be positive"));
    }
    if map_def.levels.is_empty() {
        return Err(anyhow!("at least one level is required"));
    }
    if map_def.player_spawn_fields.is_empty() {
        return Err(anyhow!("at least one player_spawn_fields entry is required"));
    }

    let spawn_fields = validate_spawn_fields(map_def)?;
    validate_levels(map_def)?;
    validate_spawn_fields_have_floors(map_def, &spawn_fields)?;
    validate_ramps(map_def, &spawn_fields)?;

    Ok(())
}

fn validate_spawn_fields(map_def: &MapDef) -> Result<BTreeSet<PlayerSpawnDef>> {
    let mut spawn_fields = BTreeSet::new();
    for (spawn_idx, field) in map_def.player_spawn_fields.iter().enumerate() {
        if field.level as usize >= map_def.levels.len() {
            return Err(anyhow!(
                "player_spawn_fields[{spawn_idx}] level {} out of range (level count = {})",
                field.level,
                map_def.levels.len()
            ));
        }
        validate_floor(field.point(), map_def.grid_cols, map_def.grid_rows)
            .with_context(|| format!("player_spawn_fields[{spawn_idx}]"))?;
        if !spawn_fields.insert(*field) {
            return Err(anyhow!("duplicate player_spawn_fields {:?}", field));
        }
    }
    Ok(spawn_fields)
}

fn validate_levels(map_def: &MapDef) -> Result<()> {
    for (level_idx, level) in map_def.levels.iter().enumerate() {
        let label = level_label(level_idx, level);
        if level.floors.is_empty() {
            return Err(anyhow!("{label}: at least one floor is required"));
        }

        let floors = validate_regular_floors(level, &label, map_def.grid_cols, map_def.grid_rows)?;
        validate_inaccessible_floors(level, &label, map_def.grid_cols, map_def.grid_rows, &floors)?;
        validate_walls(level, &label, map_def.grid_cols, map_def.grid_rows)?;
    }
    Ok(())
}

fn validate_regular_floors(
    level: &LevelDef,
    label: &str,
    grid_cols: i32,
    grid_rows: i32,
) -> Result<BTreeSet<[i32; 2]>> {
    let mut floors = BTreeSet::new();
    for (floor_idx, floor) in level.floors.iter().enumerate() {
        validate_floor(*floor, grid_cols, grid_rows).with_context(|| format!("{label}: floors[{floor_idx}]"))?;
        if !floors.insert(*floor) {
            return Err(anyhow!("{label}: duplicate floor {:?}", floor));
        }
    }
    Ok(floors)
}

fn validate_inaccessible_floors(
    level: &LevelDef,
    label: &str,
    grid_cols: i32,
    grid_rows: i32,
    floors: &BTreeSet<[i32; 2]>,
) -> Result<()> {
    let mut inaccessible_floors = BTreeSet::new();
    for (floor_idx, floor) in level.inaccessible_floors.iter().enumerate() {
        validate_floor(*floor, grid_cols, grid_rows)
            .with_context(|| format!("{label}: inaccessible_floors[{floor_idx}]"))?;
        if !inaccessible_floors.insert(*floor) {
            return Err(anyhow!("{label}: duplicate inaccessible_floor {:?}", floor));
        }
        if floors.contains(floor) {
            return Err(anyhow!("{label}: inaccessible_floor {:?} overlaps a floor", floor));
        }
    }
    Ok(())
}

fn validate_walls(level: &LevelDef, label: &str, grid_cols: i32, grid_rows: i32) -> Result<()> {
    let mut walls_seen = BTreeSet::new();
    for (wall_idx, wall) in level.walls.iter().enumerate() {
        validate_wall(*wall, grid_cols, grid_rows).with_context(|| format!("{label}: walls[{wall_idx}]"))?;
        let wall = normalized_wall(*wall);
        if !walls_seen.insert(wall) {
            return Err(anyhow!("{label}: duplicate wall {:?}", wall));
        }
    }
    Ok(())
}

fn validate_spawn_fields_have_floors(map_def: &MapDef, spawn_fields: &BTreeSet<PlayerSpawnDef>) -> Result<()> {
    let floor_sets: Vec<BTreeSet<[i32; 2]>> = map_def
        .levels
        .iter()
        .map(|level| level.floors.iter().copied().collect())
        .collect();
    for field in spawn_fields {
        if !floor_sets[field.level as usize].contains(&field.point()) {
            return Err(anyhow!(
                "player_spawn_fields {:?} is not a floor on level {}",
                field.point(),
                field.level
            ));
        }
    }
    Ok(())
}

fn validate_ramps(map_def: &MapDef, spawn_fields: &BTreeSet<PlayerSpawnDef>) -> Result<()> {
    let mut ramps_seen = BTreeSet::new();
    for (idx, ramp) in map_def.ramps.iter().enumerate() {
        validate_ramp(ramp, map_def.grid_cols, map_def.grid_rows, map_def.levels.len())
            .with_context(|| format!("ramps[{idx}]"))?;
        let key = (ramp.lower_level, ramp.low, ramp.high);
        if !ramps_seen.insert(key) {
            return Err(anyhow!("duplicate ramp {:?}", key));
        }
        validate_ramp_spawn_overlap(ramp, spawn_fields)?;
    }
    Ok(())
}

fn validate_ramp_spawn_overlap(ramp: &RampDef, spawn_fields: &BTreeSet<PlayerSpawnDef>) -> Result<()> {
    for cell in ramp_footprint_cells(ramp) {
        for level in [ramp.lower_level, ramp.lower_level + 1] {
            if spawn_fields.contains(&PlayerSpawnDef {
                level,
                col: cell[0],
                row: cell[1],
            }) {
                return Err(anyhow!(
                    "player_spawn_fields {:?} overlaps a ramp on level {}",
                    cell,
                    level
                ));
            }
        }
    }
    Ok(())
}

fn level_label(level_idx: usize, level: &LevelDef) -> String {
    match &level.name {
        Some(name) if !name.is_empty() => format!("level {level_idx} ({name})"),
        _ => format!("level {level_idx}"),
    }
}

fn validate_floor(floor: [i32; 2], grid_cols: i32, grid_rows: i32) -> Result<()> {
    let [col, row] = floor;
    if col < 0 || col >= grid_cols || row < 0 || row >= grid_rows {
        return Err(anyhow!(
            "floor {:?} out of grid bounds {}x{}",
            floor,
            grid_cols,
            grid_rows
        ));
    }
    Ok(())
}

fn validate_grid_point(point: [i32; 2], grid_cols: i32, grid_rows: i32) -> Result<()> {
    let [col, row] = point;
    if col < 0 || col > grid_cols || row < 0 || row > grid_rows {
        return Err(anyhow!(
            "grid point {:?} out of grid-line bounds {}x{}",
            point,
            grid_cols,
            grid_rows
        ));
    }
    Ok(())
}

fn validate_wall(wall: [i32; 4], grid_cols: i32, grid_rows: i32) -> Result<()> {
    let [c0, r0, c1, r1] = wall;
    validate_grid_point([c0, r0], grid_cols, grid_rows)?;
    validate_grid_point([c1, r1], grid_cols, grid_rows)?;

    let length = (c1 - c0).abs() + (r1 - r0).abs();
    if length != 1 {
        return Err(anyhow!("wall {:?} must be exactly one grid edge", wall));
    }
    Ok(())
}

fn validate_ramp(ramp: &RampDef, grid_cols: i32, grid_rows: i32, level_count: usize) -> Result<()> {
    if ramp.lower_level as usize + 1 >= level_count {
        return Err(anyhow!(
            "lower_level {} has no upper level (level count = {})",
            ramp.lower_level,
            level_count
        ));
    }
    validate_grid_point(ramp.low, grid_cols, grid_rows).context("low")?;
    validate_grid_point(ramp.high, grid_cols, grid_rows).context("high")?;

    let width = (ramp.high[0] - ramp.low[0]).abs();
    let height = (ramp.high[1] - ramp.low[1]).abs();
    if width == 0 || height == 0 {
        return Err(anyhow!("low/high must span a non-empty rectangular footprint"));
    }
    if width == height {
        return Err(anyhow!("ramp footprint must have one clear longer axis"));
    }
    Ok(())
}

fn ramp_footprint_cells(ramp: &RampDef) -> Vec<[i32; 2]> {
    let col_min = ramp.low[0].min(ramp.high[0]);
    let col_max = ramp.low[0].max(ramp.high[0]);
    let row_min = ramp.low[1].min(ramp.high[1]);
    let row_max = ramp.low[1].max(ramp.high[1]);

    let mut cells = Vec::new();
    for row in row_min..row_max {
        for col in col_min..col_max {
            cells.push([col, row]);
        }
    }
    cells
}

pub(super) fn canonicalize(map_def: &mut MapDef) {
    map_def
        .player_spawn_fields
        .sort_by_key(|field| (field.level, field.row, field.col));
    map_def.player_spawn_fields.dedup();

    for level in &mut map_def.levels {
        level.floors.sort_by_key(|[col, row]| (*row, *col));
        level.floors.dedup();
        level.inaccessible_floors.sort_by_key(|[col, row]| (*row, *col));
        level.inaccessible_floors.dedup();

        for wall in &mut level.walls {
            *wall = normalized_wall(*wall);
        }
        level.walls.sort();
        level.walls.dedup();
    }

    map_def.ramps.sort_by_key(|r| (r.lower_level, r.low, r.high));
    map_def.ramps.dedup_by_key(|r| (r.lower_level, r.low, r.high));
}

fn normalized_wall(wall: [i32; 4]) -> [i32; 4] {
    let [c0, r0, c1, r1] = wall;
    if (c1, r1) < (c0, r0) { [c1, r1, c0, r0] } else { wall }
}
