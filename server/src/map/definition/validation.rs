use std::collections::BTreeSet;

use anyhow::{Context, Result, anyhow};

use common::protocol::ItemType;

use super::schema::{ActorSpawnZoneDef, LadderDef, LevelDef, MapDef, PlayerSpawnZoneDef, RampDef, WallSide};

pub(super) fn validate_map(map_def: &MapDef) -> Result<()> {
    if map_def.grid_cols <= 0 || map_def.grid_rows <= 0 {
        return Err(anyhow!("grid_cols and grid_rows must be positive"));
    }
    if map_def.levels.is_empty() {
        return Err(anyhow!("at least one level is required"));
    }
    if map_def.player_spawn_zones.is_empty() {
        return Err(anyhow!("at least one player_spawn_zones entry is required"));
    }

    validate_actor_spawn_zones(map_def)?;
    validate_player_spawn_zones(map_def)?;
    validate_items(map_def)?;
    validate_pressure_plates(map_def)?;
    validate_levels(map_def)?;
    validate_ramps(map_def)?;
    validate_ladders(map_def)?;

    Ok(())
}

// Common rect-shaped accessors that both actor and player zones share.
// Used by the level-bounds and range checks below.
trait ZoneRect {
    fn level(&self) -> u32;
    fn cols(&self) -> [i32; 2];
    fn rows(&self) -> [i32; 2];
}

impl ZoneRect for ActorSpawnZoneDef {
    fn level(&self) -> u32 {
        self.level
    }
    fn cols(&self) -> [i32; 2] {
        self.cols
    }
    fn rows(&self) -> [i32; 2] {
        self.rows
    }
}

impl ZoneRect for PlayerSpawnZoneDef {
    fn level(&self) -> u32 {
        self.level
    }
    fn cols(&self) -> [i32; 2] {
        self.cols
    }
    fn rows(&self) -> [i32; 2] {
        self.rows
    }
}

fn validate_actor_spawn_zones(map_def: &MapDef) -> Result<()> {
    for (zone_idx, zone) in map_def.actor_spawn_zones.iter().enumerate() {
        let label = format!("actor_spawn_zones[{zone_idx}]");
        validate_zone_placement(zone, &label, map_def)?;
        if zone.kind.is_empty() {
            return Err(anyhow!("{label} has empty `kind`"));
        }
    }
    Ok(())
}

fn validate_player_spawn_zones(map_def: &MapDef) -> Result<()> {
    for (zone_idx, zone) in map_def.player_spawn_zones.iter().enumerate() {
        let label = format!("player_spawn_zones[{zone_idx}]");
        validate_zone_placement(zone, &label, map_def)?;
    }
    Ok(())
}

fn validate_items(map_def: &MapDef) -> Result<()> {
    let mut seen = BTreeSet::new();
    for (idx, item) in map_def.items.iter().enumerate() {
        let label = format!("items[{idx}]");
        if item.level as usize >= map_def.levels.len() {
            return Err(anyhow!(
                "{label} level {} out of range (level count = {})",
                item.level,
                map_def.levels.len()
            ));
        }
        if item.col < 0 || item.col >= map_def.grid_cols {
            return Err(anyhow!(
                "{label} col {} out of range [0, {})",
                item.col,
                map_def.grid_cols
            ));
        }
        if item.row < 0 || item.row >= map_def.grid_rows {
            return Err(anyhow!(
                "{label} row {} out of range [0, {})",
                item.row,
                map_def.grid_rows
            ));
        }
        if item.item_type == ItemType::KEY_CONFIG_ID {
            if item.kind.as_ref().is_none_or(String::is_empty) {
                return Err(anyhow!("{label} is a key but has no `kind`"));
            }
        } else {
            if ItemType::from_config_id(&item.item_type).is_none() {
                return Err(anyhow!("{label} has unknown item type {:?}", item.item_type));
            }
            if item.kind.is_some() {
                return Err(anyhow!(
                    "{label} ({}) must not have `kind` — only key items take one",
                    item.item_type
                ));
            }
        }
        // Unlike pressure plates there's no per-kind exemption: two items on
        // one cell would physically stack.
        if !seen.insert((item.level, item.col, item.row)) {
            return Err(anyhow!(
                "{label} duplicates an item at level {} col {} row {}",
                item.level,
                item.col,
                item.row
            ));
        }
    }
    Ok(())
}

fn validate_pressure_plates(map_def: &MapDef) -> Result<()> {
    let mut seen = BTreeSet::new();
    for (idx, plate) in map_def.pressure_plates.iter().enumerate() {
        let label = format!("pressure_plates[{idx}]");
        if plate.level as usize >= map_def.levels.len() {
            return Err(anyhow!(
                "{label} level {} out of range (level count = {})",
                plate.level,
                map_def.levels.len()
            ));
        }
        if plate.col < 0 || plate.col >= map_def.grid_cols {
            return Err(anyhow!(
                "{label} col {} out of range [0, {})",
                plate.col,
                map_def.grid_cols
            ));
        }
        if plate.row < 0 || plate.row >= map_def.grid_rows {
            return Err(anyhow!(
                "{label} row {} out of range [0, {})",
                plate.row,
                map_def.grid_rows
            ));
        }
        // A plate needs a floor to sit on, and a light bridge is not one.
        if map_def.levels[plate.level as usize]
            .light_bridges
            .iter()
            .any(|bridge| bridge.col == plate.col && bridge.row == plate.row)
        {
            return Err(anyhow!(
                "{label} sits on a light bridge at level {} col {} row {}",
                plate.level,
                plate.col,
                plate.row
            ));
        }
        // Per-purpose uniqueness on (level, col, row). Different purposes
        // may stack on the same cell — design choice; drop the purpose from
        // the tuple to disallow that too.
        if !seen.insert((plate.level, plate.col, plate.row, plate.purpose.clone())) {
            return Err(anyhow!(
                "{label} duplicates {:?} at level {} col {} row {}",
                plate.purpose,
                plate.level,
                plate.col,
                plate.row
            ));
        }
    }
    Ok(())
}

fn validate_zone_placement<Z: ZoneRect>(zone: &Z, label: &str, map_def: &MapDef) -> Result<()> {
    if zone.level() as usize >= map_def.levels.len() {
        return Err(anyhow!(
            "{label} level {} out of range (level count = {})",
            zone.level(),
            map_def.levels.len()
        ));
    }
    validate_zone_range(zone, map_def.grid_cols, map_def.grid_rows).with_context(|| label.to_string())
}

fn validate_zone_range<Z: ZoneRect>(zone: &Z, grid_cols: i32, grid_rows: i32) -> Result<()> {
    let [c0, c1] = zone.cols();
    let [r0, r1] = zone.rows();
    if c1 <= c0 || r1 <= r0 {
        return Err(anyhow!(
            "zone range must be non-empty (cols={:?}, rows={:?})",
            zone.cols(),
            zone.rows()
        ));
    }
    if c0 < 0 || c1 > grid_cols || r0 < 0 || r1 > grid_rows {
        return Err(anyhow!(
            "zone range out of grid bounds {grid_cols}x{grid_rows} (cols={:?}, rows={:?})",
            zone.cols(),
            zone.rows()
        ));
    }
    Ok(())
}

fn validate_levels(map_def: &MapDef) -> Result<()> {
    for (level_idx, level) in map_def.levels.iter().enumerate() {
        let label = level_label(level_idx, level);
        if level.floors.is_empty() {
            return Err(anyhow!("{label}: at least one floor is required"));
        }

        let floors = validate_regular_floors(level, &label, map_def.grid_cols, map_def.grid_rows)?;
        let inaccessible = validate_inaccessible_floors(level, &label, map_def.grid_cols, map_def.grid_rows, &floors)?;
        validate_grass(level, &label, map_def.grid_cols, map_def.grid_rows)?;
        let walls = validate_walls(level, &label, map_def.grid_cols, map_def.grid_rows)?;
        validate_barriers(level, &label, map_def.grid_cols, map_def.grid_rows, &walls)?;
        validate_light_bridges(level, &label, map_def, level_idx, &floors, &inaccessible)?;
    }
    Ok(())
}

// A light bridge spans a gap, so it may not share a cell with anything a
// player already stands on.
fn validate_light_bridges(
    level: &LevelDef,
    label: &str,
    map_def: &MapDef,
    level_idx: usize,
    floors: &BTreeSet<[i32; 2]>,
    inaccessible: &BTreeSet<[i32; 2]>,
) -> Result<()> {
    if level.light_bridges.is_empty() {
        return Ok(());
    }
    let ramps = ramp_cells_on_level(map_def, level_idx);
    let mut seen = BTreeSet::new();
    for (idx, bridge) in level.light_bridges.iter().enumerate() {
        let key = [bridge.col, bridge.row];
        validate_floor(key, map_def.grid_cols, map_def.grid_rows)
            .with_context(|| format!("{label}: light_bridges[{idx}]"))?;
        if bridge.kind.is_empty() {
            return Err(anyhow!("{label}: light_bridges[{idx}] has empty `kind`"));
        }
        if floors.contains(&key) || inaccessible.contains(&key) {
            return Err(anyhow!("{label}: light_bridges[{idx}] {key:?} sits on a floor"));
        }
        if ramps.contains(&key) {
            return Err(anyhow!("{label}: light_bridges[{idx}] {key:?} sits on a ramp"));
        }
        if !seen.insert(key) {
            return Err(anyhow!("{label}: duplicate light_bridge {key:?}"));
        }
    }
    Ok(())
}

// Footprint cells of every ramp touching a level; a ramp occupies its cells
// on both the level it leaves and the one it reaches.
fn ramp_cells_on_level(map_def: &MapDef, level_idx: usize) -> BTreeSet<[i32; 2]> {
    let mut cells = BTreeSet::new();
    for ramp in &map_def.ramps {
        let lower = ramp.lower_level as usize;
        if level_idx != lower && level_idx != lower + 1 {
            continue;
        }
        for row in ramp.low[1].min(ramp.high[1])..ramp.low[1].max(ramp.high[1]) {
            for col in ramp.low[0].min(ramp.high[0])..ramp.low[0].max(ramp.high[0]) {
                cells.insert([col, row]);
            }
        }
    }
    cells
}

fn validate_regular_floors(
    level: &LevelDef,
    label: &str,
    grid_cols: i32,
    grid_rows: i32,
) -> Result<BTreeSet<[i32; 2]>> {
    let mut floors = BTreeSet::new();
    for (floor_idx, floor) in level.floors.iter().enumerate() {
        let key = [floor.col, floor.row];
        validate_floor(key, grid_cols, grid_rows).with_context(|| format!("{label}: floors[{floor_idx}]"))?;
        if !floors.insert(key) {
            return Err(anyhow!("{label}: duplicate floor {:?}", key));
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
) -> Result<BTreeSet<[i32; 2]>> {
    let mut inaccessible_floors = BTreeSet::new();
    for (floor_idx, floor) in level.inaccessible_floors.iter().enumerate() {
        let key = [floor.col, floor.row];
        validate_floor(key, grid_cols, grid_rows)
            .with_context(|| format!("{label}: inaccessible_floors[{floor_idx}]"))?;
        if !inaccessible_floors.insert(key) {
            return Err(anyhow!("{label}: duplicate inaccessible_floor {:?}", key));
        }
        if floors.contains(&key) {
            return Err(anyhow!("{label}: inaccessible_floor {:?} overlaps a floor", key));
        }
    }
    Ok(inaccessible_floors)
}

// No floor-presence check here: compile silently drops grass on floorless
// cells (defense-in-depth for hand-edited JSON; the editor is the primary
// enforcement).
fn validate_grass(level: &LevelDef, label: &str, grid_cols: i32, grid_rows: i32) -> Result<()> {
    let mut grass = BTreeSet::new();
    for (grass_idx, cell) in level.grass.iter().enumerate() {
        let key = [cell.col, cell.row];
        validate_floor(key, grid_cols, grid_rows).with_context(|| format!("{label}: grass[{grass_idx}]"))?;
        if !grass.insert(key) {
            return Err(anyhow!("{label}: duplicate grass {:?}", key));
        }
    }
    Ok(())
}

fn validate_walls(level: &LevelDef, label: &str, grid_cols: i32, grid_rows: i32) -> Result<BTreeSet<[i32; 4]>> {
    let mut walls_seen = BTreeSet::new();
    for (wall_idx, wall) in level.walls.iter().enumerate() {
        let key = [wall.c0, wall.r0, wall.c1, wall.r1];
        validate_wall(key, grid_cols, grid_rows).with_context(|| format!("{label}: walls[{wall_idx}]"))?;
        let normalized = normalized_wall(key);
        if !walls_seen.insert(normalized) {
            return Err(anyhow!("{label}: duplicate wall {:?}", normalized));
        }
    }
    Ok(walls_seen)
}

// Barriers share the wall edge geometry but must not coexist with a wall on
// the same edge (visual z-fight + ambiguous semantics).
fn validate_barriers(
    level: &LevelDef,
    label: &str,
    grid_cols: i32,
    grid_rows: i32,
    walls: &BTreeSet<[i32; 4]>,
) -> Result<()> {
    let mut barriers_seen = BTreeSet::new();
    for (barrier_idx, barrier) in level.barriers.iter().enumerate() {
        let key = [barrier.c0, barrier.r0, barrier.c1, barrier.r1];
        validate_wall(key, grid_cols, grid_rows).with_context(|| format!("{label}: barriers[{barrier_idx}]"))?;
        let normalized = normalized_wall(key);
        if walls.contains(&normalized) {
            return Err(anyhow!(
                "{label}: barriers[{barrier_idx}] {:?} overlaps a wall on the same edge",
                normalized
            ));
        }
        if !barriers_seen.insert(normalized) {
            return Err(anyhow!("{label}: duplicate barrier {:?}", normalized));
        }
    }
    Ok(())
}

fn validate_ramps(map_def: &MapDef) -> Result<()> {
    let mut ramps_seen = BTreeSet::new();
    for (idx, ramp) in map_def.ramps.iter().enumerate() {
        validate_ramp(ramp, map_def.grid_cols, map_def.grid_rows, map_def.levels.len())
            .with_context(|| format!("ramps[{idx}]"))?;
        let key = (ramp.lower_level, ramp.low, ramp.high);
        if !ramps_seen.insert(key) {
            return Err(anyhow!("duplicate ramp {:?}", key));
        }
    }
    Ok(())
}

// Ladders are deliberately permissive — no wall, floor, or clear-edge
// requirements (the climb mechanic handles every surrounding, and jumping
// off mid-climb is always possible). Only structural integrity is checked.
// Overlap is keyed on the undirected edge: an edge holds at most one
// ladder — a mirrored pair would put two ladders' geometry on the same
// edge, so it counts as the same ladder twice even though only the front
// side climbs.
fn validate_ladders(map_def: &MapDef) -> Result<()> {
    for (idx, ladder) in map_def.ladders.iter().enumerate() {
        validate_ladder(ladder, map_def.grid_cols, map_def.grid_rows, map_def.levels.len())
            .with_context(|| format!("ladders[{idx}]"))?;
        let overlapping = map_def.ladders[..idx].iter().any(|other| {
            ladder_edge(other) == ladder_edge(ladder)
                && other.lower_level < ladder.lower_level + ladder.levels
                && ladder.lower_level < other.lower_level + other.levels
        });
        if overlapping {
            return Err(anyhow!(
                "ladders[{idx}] overlaps another ladder on the same edge (col {}, row {}, side {:?})",
                ladder.col,
                ladder.row,
                ladder.side
            ));
        }
    }
    Ok(())
}

fn ladder_edge(ladder: &LadderDef) -> [(i32, i32); 2] {
    let (col, row) = (ladder.col, ladder.row);
    match ladder.side {
        WallSide::North => [(col, row), (col + 1, row)],
        WallSide::South => [(col, row + 1), (col + 1, row + 1)],
        WallSide::West => [(col, row), (col, row + 1)],
        WallSide::East => [(col + 1, row), (col + 1, row + 1)],
    }
}

fn validate_ladder(ladder: &LadderDef, grid_cols: i32, grid_rows: i32, level_count: usize) -> Result<()> {
    if ladder.col < 0 || ladder.col >= grid_cols || ladder.row < 0 || ladder.row >= grid_rows {
        return Err(anyhow!(
            "cell (col {}, row {}) out of grid bounds {grid_cols}x{grid_rows}",
            ladder.col,
            ladder.row
        ));
    }
    if ladder.levels == 0 {
        return Err(anyhow!("levels must be at least 1"));
    }
    let top_level = ladder.lower_level as usize + ladder.levels as usize;
    if top_level >= level_count {
        return Err(anyhow!(
            "spans up to level {top_level} which does not exist (level count = {level_count})"
        ));
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

pub(super) fn canonicalize(map_def: &mut MapDef) {
    map_def.actor_spawn_zones.sort_by(|a, b| {
        (a.level, a.rows[0], a.cols[0], a.rows[1], a.cols[1], &a.kind, a.count)
            .cmp(&(b.level, b.rows[0], b.cols[0], b.rows[1], b.cols[1], &b.kind, b.count))
    });
    map_def.actor_spawn_zones.dedup();

    map_def
        .player_spawn_zones
        .sort_by_key(|z| (z.level, z.rows[0], z.cols[0], z.rows[1], z.cols[1]));
    map_def.player_spawn_zones.dedup();

    for level in &mut map_def.levels {
        level.floors.sort_by_key(|f| (f.row, f.col));
        level.floors.dedup_by_key(|f| (f.row, f.col));
        level.inaccessible_floors.sort_by_key(|f| (f.row, f.col));
        level.inaccessible_floors.dedup_by_key(|f| (f.row, f.col));
        level.grass.sort_by_key(|f| (f.row, f.col));
        level.grass.dedup_by_key(|f| (f.row, f.col));

        for wall in &mut level.walls {
            let [c0, r0, c1, r1] = normalized_wall([wall.c0, wall.r0, wall.c1, wall.r1]);
            wall.c0 = c0;
            wall.r0 = r0;
            wall.c1 = c1;
            wall.r1 = r1;
        }
        level.walls.sort_by_key(|w| (w.c0, w.r0, w.c1, w.r1));
        level.walls.dedup_by_key(|w| (w.c0, w.r0, w.c1, w.r1));
    }

    map_def.ramps.sort_by_key(|r| (r.lower_level, r.low, r.high));
    map_def.ramps.dedup_by_key(|r| (r.lower_level, r.low, r.high));

    map_def
        .ladders
        .sort_by_key(|l| (l.lower_level, l.row, l.col, l.side as u8, l.levels));
    map_def.ladders.dedup();
}

fn normalized_wall(wall: [i32; 4]) -> [i32; 4] {
    let [c0, r0, c1, r1] = wall;
    if (c1, r1) < (c0, r0) { [c1, r1, c0, r0] } else { wall }
}
