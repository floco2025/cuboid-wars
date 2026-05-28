use anyhow::Context;

use super::schema::{BarrierDef, MapDef, RampDef};
use crate::{
    map::{
        barriers::merge_barriers,
        floors,
        lights::generate_wall_lights,
        mask::{Mask, mark_has_floor, mark_has_floor_above, mark_has_floor_slab},
        material_rules::MaterialRules,
        ramps, trim, walls,
    },
    resources::{
        ActorSpawnZone, CellGrid, CookieSpawnZone, EdgeGrid, KeySpawnZone, LevelGrid, MapConfig, PlayerSpawnZone,
        PressurePlateRuntime,
    },
};
use common::{
    constants::*,
    face_materials::FaceMaterials,
    map_geometry::MapGeometry,
    protocol::{Barrier, BarrierKindTable, Floor, MapLayout, Wall},
};

pub(crate) fn compile_map(
    map_def: &MapDef,
    assets: &MaterialRules,
    kind_table: &BarrierKindTable,
) -> anyhow::Result<(MapLayout, MapConfig, MapGeometry)> {
    let cols = map_def.grid_cols;
    let rows = map_def.grid_rows;
    let geometry = MapGeometry::new(cols, rows);

    let ramp_specs: Vec<ramps::RampSpec> = map_def.ramps.iter().map(ramp_spec_from_def).collect();

    let regular_floor_masks: Vec<Mask> = map_def
        .levels
        .iter()
        .map(|level| {
            let mut m = empty_mask(cols, rows);
            for floor in &level.floors {
                m[floor.row as usize][floor.col as usize] = true;
            }
            m
        })
        .collect();
    let slab_masks: Vec<Mask> = map_def
        .levels
        .iter()
        .map(|level| {
            let mut m = empty_mask(cols, rows);
            for floor in level.floors.iter().chain(level.inaccessible_floors.iter()) {
                m[floor.row as usize][floor.col as usize] = true;
            }
            m
        })
        .collect();

    let mut level_grids: Vec<LevelGrid> = map_def
        .levels
        .iter()
        .enumerate()
        .map(|(level_idx, level)| {
            let mut cell_grid = CellGrid::new(cols, rows);
            let mut edge_grid = EdgeGrid::new(cols, rows);
            mark_has_floor(&mut cell_grid, &regular_floor_masks[level_idx]);
            mark_has_floor_slab(&mut cell_grid, &slab_masks[level_idx]);
            for wall in &level.walls {
                set_wall_edge(&mut edge_grid, [wall.c0, wall.r0, wall.c1, wall.r1]);
            }
            LevelGrid {
                cells: cell_grid,
                edges: edge_grid,
            }
        })
        .collect();

    for (level_idx, level_grid) in level_grids.iter_mut().enumerate() {
        let level_u32 = u32::try_from(level_idx).unwrap_or(u32::MAX);
        ramps::apply_to_level_cells(&mut level_grid.cells, &ramp_specs, level_u32);
    }
    for level_idx in 0..level_grids.len().saturating_sub(1) {
        mark_has_floor_above(&mut level_grids[level_idx].cells, &slab_masks[level_idx + 1]);
    }

    let mut wall_lights = Vec::new();
    for (level_idx, level_grid) in level_grids.iter().enumerate() {
        wall_lights.extend(generate_wall_lights(
            &geometry,
            level_grid,
            level_idx,
            &map_def.levels[level_idx].lights,
        ));
    }

    let mut all_walls: Vec<Wall> = Vec::new();
    let mut all_wall_materials: Vec<FaceMaterials> = Vec::new();
    for (level_idx, level_grid) in level_grids.iter().enumerate() {
        let level_u8 = u8::try_from(level_idx).unwrap_or(u8::MAX);
        let tier = walls::generate_walls(&level_grid.edges, &geometry, level_u8);
        let (merged_walls, merged_materials) = walls::merge_walls(tier, assets);
        all_walls.extend(merged_walls);
        all_wall_materials.extend(merged_materials);
    }

    let mut all_barriers: Vec<Barrier> = Vec::new();
    for (level_idx, level) in map_def.levels.iter().enumerate() {
        let level_u8 = u8::try_from(level_idx).unwrap_or(u8::MAX);
        for (barrier_idx, b) in level.barriers.iter().enumerate() {
            all_barriers.push(
                barrier_from_def(b, &geometry, level_u8, kind_table)
                    .with_context(|| format!("level {level_idx} barriers[{barrier_idx}]"))?,
            );
        }
    }
    let all_barriers = merge_barriers(all_barriers);

    let mut all_floors: Vec<Floor> = Vec::new();
    let mut all_floor_materials: Vec<FaceMaterials> = Vec::new();
    for (level_idx, m) in slab_masks.iter().enumerate() {
        let level_u8 = u8::try_from(level_idx).unwrap_or(u8::MAX);
        let y = f32::from(level_u8) * LEVEL_HEIGHT;
        // Tell floor emission to skip its corner-filler strip at the high
        // end of each z-axis ramp arriving at this level — a strip there
        // would hover above where the slope already meets the upper floor.
        let mut skip_corner_filler_edges: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
        for ramp in &ramp_specs {
            if ramp.lower_level + 1 == level_idx as u32 {
                skip_corner_filler_edges.extend(ramp.high_end_horizontal_edges());
            }
        }
        let mut tier = floors::emit_floor_tier(m, &skip_corner_filler_edges, &geometry, level_u8, y);
        if level_idx > 0 {
            tier.extend(trim::emit_stacked_wall_trim(
                &level_grids[level_idx - 1].edges,
                &level_grids[level_idx].edges,
                m,
                &geometry,
                level_u8,
                y,
            ));
        }
        let (merged_floors, merged_materials) = floors::merge_floors(tier, assets);
        all_floors.extend(merged_floors);
        all_floor_materials.extend(merged_materials);
    }

    let ramps_out = ramps::specs_to_ramps(&geometry, &ramp_specs);
    let ramp_materials: Vec<FaceMaterials> = ramps_out.iter().map(|r| assets.materials_for_ramp_top(r)).collect();

    let pressure_plates = pressure_plates(map_def, kind_table)?;
    let map_layout = MapLayout {
        walls: all_walls,
        wall_materials: all_wall_materials,
        ramps: ramps_out,
        ramp_materials,
        wall_lights,
        floors: all_floors,
        floor_materials: all_floor_materials,
        barriers: all_barriers,
        pressure_plates: pressure_plates
            .iter()
            .map(|p| common::protocol::PressurePlate {
                level: p.level,
                center_x: geometry.cell_to_world_x(p.col) + GRID_CELL_SIZE / 2.0,
                center_z: geometry.cell_to_world_z(p.row) + GRID_CELL_SIZE / 2.0,
                kind: p.kind,
            })
            .collect(),
    };
    // The renderer indexes the material vectors by segment position, so any
    // length divergence is a bug here, not in the client.
    assert_eq!(map_layout.walls.len(), map_layout.wall_materials.len());
    assert_eq!(map_layout.floors.len(), map_layout.floor_materials.len());
    assert_eq!(map_layout.ramps.len(), map_layout.ramp_materials.len());

    Ok((
        map_layout,
        MapConfig {
            levels: level_grids,
            actor_spawn_zones: actor_spawn_zones(map_def),
            player_spawn_zones: player_spawn_zones(map_def),
            cookie_spawn_zones: cookie_spawn_zones(map_def),
            key_spawn_zones: key_spawn_zones(map_def, kind_table)?,
            pressure_plates,
        },
        geometry,
    ))
}

fn ramp_spec_from_def(r: &RampDef) -> ramps::RampSpec {
    ramps::RampSpec {
        lower_level: r.lower_level,
        low: r.low,
        high: r.high,
    }
}

fn empty_mask(grid_cols: i32, grid_rows: i32) -> Mask {
    vec![vec![false; grid_cols as usize]; grid_rows as usize]
}

fn actor_spawn_zones(map_def: &MapDef) -> Vec<ActorSpawnZone> {
    map_def
        .actor_spawn_zones
        .iter()
        .map(|zone| ActorSpawnZone {
            level: u8::try_from(zone.level).unwrap_or(u8::MAX),
            cols: zone.cols,
            rows: zone.rows,
            kind: zone.kind.clone(),
            count: zone.count,
        })
        .collect()
}

fn player_spawn_zones(map_def: &MapDef) -> Vec<PlayerSpawnZone> {
    map_def
        .player_spawn_zones
        .iter()
        .map(|zone| PlayerSpawnZone {
            level: u8::try_from(zone.level).unwrap_or(u8::MAX),
            cols: zone.cols,
            rows: zone.rows,
        })
        .collect()
}

fn cookie_spawn_zones(map_def: &MapDef) -> Vec<CookieSpawnZone> {
    map_def
        .cookie_spawn_zones
        .iter()
        .map(|zone| CookieSpawnZone {
            level: u8::try_from(zone.level).unwrap_or(u8::MAX),
            cols: zone.cols,
            rows: zone.rows,
        })
        .collect()
}

fn pressure_plates(map_def: &MapDef, kind_table: &BarrierKindTable) -> anyhow::Result<Vec<PressurePlateRuntime>> {
    map_def
        .pressure_plates
        .iter()
        .enumerate()
        .map(|(idx, p)| {
            let kind = kind_table
                .resolve(&p.kind)
                .with_context(|| format!("pressure_plates[{idx}]"))?;
            Ok(PressurePlateRuntime {
                level: u8::try_from(p.level).unwrap_or(u8::MAX),
                col: p.col,
                row: p.row,
                kind,
            })
        })
        .collect()
}

fn key_spawn_zones(map_def: &MapDef, kind_table: &BarrierKindTable) -> anyhow::Result<Vec<KeySpawnZone>> {
    map_def
        .key_spawn_zones
        .iter()
        .enumerate()
        .map(|(idx, zone)| {
            let kind = kind_table
                .resolve(&zone.kind)
                .with_context(|| format!("key_spawn_zones[{idx}]"))?;
            Ok(KeySpawnZone {
                level: u8::try_from(zone.level).unwrap_or(u8::MAX),
                cols: zone.cols,
                rows: zone.rows,
                kind,
            })
        })
        .collect()
}

fn set_wall_edge(edges: &mut EdgeGrid, wall: [i32; 4]) {
    let [c0, r0, c1, r1] = wall;
    if r0 == r1 {
        edges.horizontal[r0 as usize][c0.min(c1) as usize] = true;
    } else {
        edges.vertical[r0.min(r1) as usize][c0 as usize] = true;
    }
}

// Convert an editor-authored one-edge barrier into a world-space `Barrier`
// segment. Mirrors the wall world-space math (cell-corner → world-corner via
// `MapGeometry`) so a barrier visually occupies the same edge as a wall would.
fn barrier_from_def(
    def: &BarrierDef,
    geometry: &MapGeometry,
    level: u8,
    kind_table: &BarrierKindTable,
) -> anyhow::Result<Barrier> {
    let x1 = geometry.cell_to_world_x(def.c0);
    let z1 = geometry.cell_to_world_z(def.r0);
    let x2 = geometry.cell_to_world_x(def.c1);
    let z2 = geometry.cell_to_world_z(def.r1);
    let kind = kind_table.resolve(&def.kind)?;
    Ok(Barrier {
        x1,
        z1,
        x2,
        z2,
        level,
        kind,
    })
}
