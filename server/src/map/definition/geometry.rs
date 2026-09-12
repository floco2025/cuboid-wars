use std::collections::BTreeMap;

use anyhow::Context;

use super::{
    compile::{CompileOutput, CompileScope},
    schema::{FloorDef, LadderDef, MapDef, RampDef, WallSide},
};
use crate::map::{
    ActorSpawnZone, CarrierGrid, CellGrid, EdgeGrid, LevelGrid, PlacedItem, PlayerSpawnZone, PressurePlateRuntime,
    barriers::{BarrierEdge, merge_barriers, stack_barriers},
    bridge_bounds::flush_light_bridges,
    bridges::merge_light_bridges,
    floors,
    lights::generate_wall_lights,
    mask::{Mask, mark_has_floor, mark_has_floor_above, mark_has_floor_slab},
    material_rules::MaterialRules,
    ramps, trim, walls,
};
use common::{
    constants::LADDER_WIDTH,
    map::MapGeometry,
    protocol::{
        Barrier, BarrierKindTable, BridgeKindId, CarrierId, Checkpoint, Eraser, FaceMaterials, Floor, GrassCell,
        ItemType, Ladder, LightBridge, PressurePlate, Ramp, SwitchId, Wall, WallLight,
    },
};

// One map's records, each born on `carrier` in this map's own frame, appended
// to the tree's layout and config.
pub(super) fn compile_geometry(
    map_def: &MapDef,
    geometry: MapGeometry,
    scope: &CompileScope,
    carrier: CarrierId,
    out: &mut CompileOutput,
) -> anyhow::Result<()> {
    // Face materials are authored per file, against its own grid.
    let assets = MaterialRules::from_def(map_def, scope.sizes);
    let ramp_specs: Vec<ramps::RampSpec> = map_def.ramps.iter().map(ramp_spec_from_def).collect();
    let regular_floor_masks: Vec<Mask> = map_def
        .levels
        .iter()
        .map(|level| floor_mask(map_def, level.floors.iter()))
        .collect();
    let slab_masks: Vec<Mask> = map_def
        .levels
        .iter()
        .map(|level| floor_mask(map_def, level.floors.iter().chain(&level.inaccessible_floors)))
        .collect();

    let pressure_plates = pressure_plates(map_def, scope, carrier)?;
    let level_grids = compile_level_grids(
        map_def,
        scope,
        &regular_floor_masks,
        &slab_masks,
        &ramp_specs,
        &geometry,
    );
    let (walls, wall_materials) = compile_walls(&level_grids, &geometry, &assets, carrier);
    let barriers = compile_barriers(map_def, scope, &slab_masks, &geometry, carrier)?;
    let (floors, floor_materials) = compile_floors(&level_grids, &slab_masks, &ramp_specs, &geometry, &assets, carrier);
    let light_bridges = flush_light_bridges(
        compile_light_bridges(map_def, &geometry, scope, carrier)?,
        &floors,
        geometry.wall_half_thickness(),
    );
    let (ramps, ramp_materials) = compile_ramps(&ramp_specs, &geometry, &assets, carrier);
    let placed_items = placed_items(map_def, scope.kind_table, &level_grids, carrier)?;

    let layout = &mut out.layout;
    layout.checkpoints.extend(map_def.checkpoints.iter().map(|def| {
        let level = level_tag(def.zone.level as usize);
        Checkpoint {
            kind: def.kind,
            carrier,
            level,
            min_x: geometry.cell_to_world_x(def.zone.cols[0]),
            max_x: geometry.cell_to_world_x(def.zone.cols[1]),
            min_z: geometry.cell_to_world_z(def.zone.rows[0]),
            max_z: geometry.cell_to_world_z(def.zone.rows[1]),
            y: geometry.level_y(level),
        }
    }));
    layout.walls.extend(walls);
    layout.wall_materials.extend(wall_materials);
    layout.ramps.extend(ramps);
    layout.ramp_materials.extend(ramp_materials);
    layout
        .wall_lights
        .extend(compile_wall_lights(map_def, &level_grids, &geometry, carrier));
    layout.floors.extend(floors);
    layout.floor_materials.extend(floor_materials);
    layout.barriers.extend(barriers);
    layout.erasers.extend(compile_erasers(map_def, &geometry, carrier));
    layout.light_bridges.extend(light_bridges);
    layout
        .pressure_plates
        .extend(pressure_plates.iter().map(|p| PressurePlate {
            level: p.level,
            center_x: geometry.cell_center_x(p.col),
            center_y: geometry.level_y(p.level),
            center_z: geometry.cell_center_z(p.row),
            switch: p.switch,
            carrier,
        }));
    layout.ladders.extend(
        map_def
            .ladders
            .iter()
            .map(|def| ladder_from_def(def, &geometry, carrier)),
    );
    layout
        .grass
        .extend(compile_grass(map_def, &slab_masks, &geometry, carrier));

    let config = &mut out.config;
    config.grids.push(CarrierGrid::new(carrier, geometry, level_grids));
    config
        .actor_spawn_zones
        .extend(actor_spawn_zones(map_def, scope, carrier)?);
    config.player_spawn_zones.extend(player_spawn_zones(map_def, carrier));
    config.placed_items.extend(placed_items);
    config.pressure_plates.extend(pressure_plates);

    Ok(())
}

fn level_tag(level_idx: usize) -> u8 {
    u8::try_from(level_idx).unwrap_or(u8::MAX)
}

fn floor_mask<'a>(map_def: &MapDef, floors: impl Iterator<Item = &'a FloorDef>) -> Mask {
    let mut mask = empty_mask(map_def.grid_cols, map_def.grid_rows);
    for floor in floors {
        mask[floor.row as usize][floor.col as usize] = true;
    }
    mask
}

fn compile_level_grids(
    map_def: &MapDef,
    scope: &CompileScope,
    regular_floor_masks: &[Mask],
    slab_masks: &[Mask],
    ramp_specs: &[ramps::RampSpec],
    geometry: &MapGeometry,
) -> Vec<LevelGrid> {
    let cols = map_def.grid_cols;
    let rows = map_def.grid_rows;
    let mut level_grids: Vec<LevelGrid> = map_def
        .levels
        .iter()
        .enumerate()
        .map(|(level_idx, level)| {
            let mut cell_grid = CellGrid::new(cols, rows);
            let mut edge_grid = EdgeGrid::new(cols, rows);
            let mut barrier_edge_grid = EdgeGrid::new(cols, rows);
            mark_has_floor(&mut cell_grid, &regular_floor_masks[level_idx]);
            mark_has_floor_slab(&mut cell_grid, &slab_masks[level_idx]);
            for wall in &level.walls {
                set_edge(&mut edge_grid, [wall.c0, wall.r0, wall.c1, wall.r1]);
            }
            for barrier in &level.barriers {
                if !barrier
                    .switch
                    .as_deref()
                    .is_some_and(|id| scope.plated_switches.contains(id))
                {
                    set_edge(&mut barrier_edge_grid, [barrier.c0, barrier.r0, barrier.c1, barrier.r1]);
                }
            }
            LevelGrid {
                cells: cell_grid,
                edges: edge_grid,
                barrier_edges: barrier_edge_grid,
            }
        })
        .collect();

    for (level_idx, level_grid) in level_grids.iter_mut().enumerate() {
        let level_u32 = u32::try_from(level_idx).unwrap_or(u32::MAX);
        ramps::apply_to_level_cells(&mut level_grid.cells, ramp_specs, level_u32, geometry);
    }
    for level_idx in 0..level_grids.len().saturating_sub(1) {
        mark_has_floor_above(&mut level_grids[level_idx].cells, &slab_masks[level_idx + 1]);
    }
    level_grids
}

fn compile_wall_lights(
    map_def: &MapDef,
    level_grids: &[LevelGrid],
    geometry: &MapGeometry,
    carrier: CarrierId,
) -> Vec<WallLight> {
    level_grids
        .iter()
        .enumerate()
        .flat_map(|(level_idx, level_grid)| {
            generate_wall_lights(
                geometry,
                level_grid,
                level_idx,
                &map_def.levels[level_idx].lights,
                carrier,
            )
        })
        .collect()
}

fn compile_walls(
    level_grids: &[LevelGrid],
    geometry: &MapGeometry,
    assets: &MaterialRules,
    carrier: CarrierId,
) -> (Vec<Wall>, Vec<FaceMaterials>) {
    let mut all_walls = Vec::new();
    let mut all_materials = Vec::new();
    for (level_idx, level_grid) in level_grids.iter().enumerate() {
        let tier = walls::generate_walls(&level_grid.edges, geometry, level_tag(level_idx), carrier);
        let (merged_walls, merged_materials) = walls::merge_walls(tier, assets);
        all_walls.extend(merged_walls);
        all_materials.extend(merged_materials);
    }
    (all_walls, all_materials)
}

fn compile_barriers(
    map_def: &MapDef,
    scope: &CompileScope,
    slab_masks: &[Mask],
    geometry: &MapGeometry,
    carrier: CarrierId,
) -> anyhow::Result<Vec<Barrier>> {
    let mut barrier_edges: Vec<Vec<BarrierEdge>> = Vec::with_capacity(map_def.levels.len());
    for (level_idx, level) in map_def.levels.iter().enumerate() {
        let mut edges = Vec::with_capacity(level.barriers.len());
        for (barrier_idx, b) in level.barriers.iter().enumerate() {
            let kind = scope
                .kind_table
                .resolve(&b.kind)
                .with_context(|| format!("level {level_idx} barriers[{barrier_idx}]"))?;
            edges.push(BarrierEdge {
                switch: scope.target_switch(b.switch.as_deref())?,
                switch_inverted: b.switch_inverted,
                edge: [b.c0, b.r0, b.c1, b.r1],
                kind,
            });
        }
        barrier_edges.push(edges);
    }
    Ok(merge_barriers(stack_barriers(
        &barrier_edges,
        slab_masks,
        geometry,
        carrier,
    )))
}

fn compile_floors(
    level_grids: &[LevelGrid],
    slab_masks: &[Mask],
    ramp_specs: &[ramps::RampSpec],
    geometry: &MapGeometry,
    assets: &MaterialRules,
    carrier: CarrierId,
) -> (Vec<Floor>, Vec<FaceMaterials>) {
    let mut all_floors = Vec::new();
    let mut all_materials = Vec::new();
    for (level_idx, m) in slab_masks.iter().enumerate() {
        let level = level_tag(level_idx);
        let y = geometry.level_y(level);
        let mut ramp_landings = EdgeGrid::new(geometry.grid_cols, geometry.grid_rows);
        for ramp in ramp_specs {
            if ramp.lower_level + 1 == level_idx as u32 {
                ramp.mark_high_end(&mut ramp_landings);
            }
        }
        let mut tier = floors::emit_floor_tier(m, &ramp_landings, geometry, level, y, carrier);
        if level_idx > 0 {
            tier.extend(trim::emit_stacked_wall_trim(
                &level_grids[level_idx - 1].edges,
                &level_grids[level_idx].edges,
                m,
                geometry,
                level,
                y,
                carrier,
            ));
        }
        let (merged_floors, merged_materials) = floors::merge_floors(tier, assets);
        all_floors.extend(merged_floors);
        all_materials.extend(merged_materials);
    }
    (all_floors, all_materials)
}

fn compile_ramps(
    ramp_specs: &[ramps::RampSpec],
    geometry: &MapGeometry,
    assets: &MaterialRules,
    carrier: CarrierId,
) -> (Vec<Ramp>, Vec<FaceMaterials>) {
    let ramps = ramps::specs_to_ramps(geometry, ramp_specs, carrier);
    let materials = ramps.iter().map(|r| assets.materials_for_ramp_top(r)).collect();
    (ramps, materials)
}

// Grass on floorless cells is silently dropped (like out-of-place wall
// lights): the editor already enforces floor presence, and a hard error
// would brick server startup over a cosmetic feature.
fn compile_grass(map_def: &MapDef, slab_masks: &[Mask], geometry: &MapGeometry, carrier: CarrierId) -> Vec<GrassCell> {
    let mut grass = Vec::new();
    for (level_idx, level) in map_def.levels.iter().enumerate() {
        let level_u8 = level_tag(level_idx);
        let y = geometry.level_y(level_u8);
        for cell in &level.grass {
            if !slab_masks[level_idx][cell.row as usize][cell.col as usize] {
                continue;
            }
            grass.push(GrassCell {
                x: geometry.cell_center_x(cell.col),
                y,
                z: geometry.cell_center_z(cell.row),
                level: level_u8,
                carrier,
            });
        }
    }
    grass
}

fn compile_erasers(map_def: &MapDef, geometry: &MapGeometry, carrier: CarrierId) -> Vec<Eraser> {
    map_def
        .levels
        .iter()
        .enumerate()
        .flat_map(|(level_idx, def)| {
            let level = level_tag(level_idx);
            def.erasers.iter().map(move |edge| Eraser {
                x1: geometry.cell_to_world_x(edge.c0),
                z1: geometry.cell_to_world_z(edge.r0),
                x2: geometry.cell_to_world_x(edge.c1),
                z2: geometry.cell_to_world_z(edge.r1),
                width: geometry.barrier_thickness(),
                y: geometry.level_y(level),
                height: geometry.level_height(),
                level,
                carrier,
            })
        })
        .collect()
}

pub(super) fn ramp_spec_from_def(r: &RampDef) -> ramps::RampSpec {
    ramps::RampSpec {
        lower_level: r.lower_level,
        low: r.low,
        high: r.high,
    }
}

fn empty_mask(grid_cols: i32, grid_rows: i32) -> Mask {
    vec![vec![false; grid_cols as usize]; grid_rows as usize]
}

fn actor_spawn_zones(
    map_def: &MapDef,
    scope: &CompileScope,
    carrier: CarrierId,
) -> anyhow::Result<Vec<ActorSpawnZone>> {
    map_def
        .actor_spawn_zones
        .iter()
        .enumerate()
        .map(|(idx, zone)| {
            Ok(ActorSpawnZone {
                switch_inverted: zone.switch_inverted,
                carrier,
                level: u8::try_from(zone.level).unwrap_or(u8::MAX),
                levels: zone.levels as u16,
                roam_distance: zone.roam_distance,
                cols: zone.cols,
                rows: zone.rows,
                kind: zone.kind.clone(),
                count: zone.count,
                respawn_secs: zone.respawn_secs,
                switch: scope
                    .target_switch(zone.switch.as_deref())
                    .with_context(|| format!("actor_spawn_zones[{idx}]"))?,
            })
        })
        .collect()
}

fn player_spawn_zones(map_def: &MapDef, carrier: CarrierId) -> Vec<PlayerSpawnZone> {
    map_def
        .player_spawn_zones
        .iter()
        .map(|zone| PlayerSpawnZone {
            carrier,
            level: u8::try_from(zone.level).unwrap_or(u8::MAX),
            levels: zone.levels as u16,
            cols: zone.cols,
            rows: zone.rows,
        })
        .collect()
}

// Items are gameplay, not cosmetics, so a floorless or ramp cell is a hard
// error (unlike grass, which compile silently drops).
fn placed_items(
    map_def: &MapDef,
    kind_table: &BarrierKindTable,
    level_grids: &[LevelGrid],
    carrier: CarrierId,
) -> anyhow::Result<Vec<PlacedItem>> {
    map_def
        .items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let item_type = if item.item_type == ItemType::KEY_CONFIG_ID {
                let kind_id = item.kind.as_deref().unwrap_or_default();
                ItemType::Key(kind_table.resolve(kind_id).with_context(|| format!("items[{idx}]"))?)
            } else {
                ItemType::from_config_id(&item.item_type)
                    .with_context(|| format!("items[{idx}] has unknown item type {:?}", item.item_type))?
            };
            let cell = level_grids[item.level as usize].cells.rows[item.row as usize][item.col as usize];
            anyhow::ensure!(
                cell.has_floor && !cell.has_ramp,
                "items[{idx}] ({}) at level {} col {} row {} needs a floor cell without a ramp",
                item.item_type,
                item.level,
                item.col,
                item.row
            );
            Ok(PlacedItem {
                carrier,
                level: u8::try_from(item.level).unwrap_or(u8::MAX),
                col: item.col,
                row: item.row,
                item_type,
            })
        })
        .collect()
}

fn compile_light_bridges(
    map_def: &MapDef,
    geometry: &MapGeometry,
    scope: &CompileScope,
    carrier: CarrierId,
) -> anyhow::Result<Vec<LightBridge>> {
    let mut out = Vec::new();
    for (level_idx, level) in map_def.levels.iter().enumerate() {
        let level_u8 = level_tag(level_idx);
        let cells = level
            .light_bridges
            .iter()
            .enumerate()
            .map(|(idx, def)| {
                let kind = scope
                    .bridge_table
                    .resolve(&def.kind)
                    .with_context(|| format!("level {level_idx} light_bridges[{idx}]"))?;
                Ok((
                    def.col,
                    def.row,
                    kind,
                    scope.target_switch(def.switch.as_deref())?,
                    def.switch_inverted,
                ))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let mut groups = BTreeMap::<(BridgeKindId, Option<SwitchId>, bool), Vec<_>>::new();
        for (col, row, kind, switch, inverted) in cells {
            groups
                .entry((kind, switch, inverted))
                .or_default()
                .push((col, row, kind));
        }
        for ((_, switch, switch_inverted), cells) in groups {
            out.extend(merge_light_bridges(&cells).into_iter().map(|rect| LightBridge {
                id: Default::default(),
                switch,
                switch_inverted,
                x1: geometry.cell_to_world_x(rect.c0),
                z1: geometry.cell_to_world_z(rect.r0),
                x2: geometry.cell_to_world_x(rect.c1),
                z2: geometry.cell_to_world_z(rect.r1),
                y: geometry.level_y(level_u8),
                thickness: geometry.bridge_thickness(),
                level: level_u8,
                kind: rect.kind,
                carrier,
            }));
        }
    }
    Ok(out)
}

fn pressure_plates(
    map_def: &MapDef,
    scope: &CompileScope,
    carrier: CarrierId,
) -> anyhow::Result<Vec<PressurePlateRuntime>> {
    map_def
        .pressure_plates
        .iter()
        .enumerate()
        .map(|(idx, p)| {
            Ok(PressurePlateRuntime {
                carrier,
                level: level_tag(p.level as usize),
                col: p.col,
                row: p.row,
                switch: scope
                    .switch_table
                    .resolve(&p.switch)
                    .with_context(|| format!("pressure_plates[{idx}]"))?,
            })
        })
        .collect()
}

fn set_edge(edges: &mut EdgeGrid, edge: [i32; 4]) {
    let [c0, r0, c1, r1] = edge;
    if r0 == r1 {
        edges.horizontal[r0 as usize][c0.min(c1) as usize] = true;
    } else {
        edges.vertical[r0.min(r1) as usize][c0 as usize] = true;
    }
}

// Convert an editor-authored `(cell, side)` ladder into a `Ladder` in the
// map's frame: the anchor edge's span shrunk to `LADDER_WIDTH` centered on
// the edge midpoint, with the normal pointing across the edge away from the
// anchor cell (into the climb volume). Side conventions match `lights.rs`:
// North = -Z, South = +Z, West = -X, East = +X.
fn ladder_from_def(def: &LadderDef, geometry: &MapGeometry, carrier: CarrierId) -> Ladder {
    let cell_x = geometry.cell_to_world_x(def.col);
    let cell_z = geometry.cell_to_world_z(def.row);
    let center_x = geometry.cell_center_x(def.col);
    let center_z = geometry.cell_center_z(def.row);
    let half_width = LADDER_WIDTH / 2.0;
    let (x1, z1, x2, z2, nx, nz) = match def.side {
        WallSide::North => (center_x - half_width, cell_z, center_x + half_width, cell_z, 0.0, -1.0),
        WallSide::South => {
            let z = cell_z + geometry.cell_size();
            (center_x - half_width, z, center_x + half_width, z, 0.0, 1.0)
        }
        WallSide::West => (cell_x, center_z - half_width, cell_x, center_z + half_width, -1.0, 0.0),
        WallSide::East => {
            let x = cell_x + geometry.cell_size();
            (x, center_z - half_width, x, center_z + half_width, 1.0, 0.0)
        }
    };
    let level = u8::try_from(def.lower_level).unwrap_or(u8::MAX);
    let levels = u8::try_from(def.levels).unwrap_or(u8::MAX);
    Ladder {
        x1,
        z1,
        x2,
        z2,
        nx,
        nz,
        y: geometry.level_y(level),
        height: f32::from(levels) * geometry.level_height(),
        level,
        levels,
        carrier,
    }
}
