use anyhow::Context;
use bevy::math::Vec3;
use std::collections::HashSet;

use super::{
    load::LoadedMaps,
    schema::{LadderDef, MapDef, MotionDef, PressurePlatePurposeDef, RampDef, WallSide},
};
use crate::{
    map::{
        ActorSpawnZone, CarrierGrid, CellGrid, EdgeGrid, LevelGrid, MapConfig, PlacedItem, PlayerSpawnZone,
        PressurePlateRuntime,
    },
    map::{
        barriers::{BarrierEdge, merge_barriers, stack_barriers},
        bridges::merge_light_bridges,
        floors,
        lights::generate_wall_lights,
        mask::{Mask, mark_has_floor, mark_has_floor_above, mark_has_floor_slab},
        material_rules::MaterialRules,
        ramps, trim, walls,
    },
};
use common::{
    config::MapGeometryConfig,
    constants::LADDER_WIDTH,
    map::MapGeometry,
    protocol::FaceMaterials,
    protocol::{
        BarrierKindId, BarrierKindTable, BridgeKindTable, Carrier, CarrierId, Floor, GrassCell, ItemType, Ladder,
        LightBridge, MapLayout, PlatePurpose, PressurePlate, Wall, ticks_from_secs,
    },
};

// The map being played and every map it nests, into one layout and one
// config: the root's records on the world carrier, each nested map's on its
// own carrier in its own frame. `nested` holds the nested maps by name
// (`load_map_tree`).
pub(crate) fn compile_map(
    root: &MapDef,
    sizes: MapGeometryConfig,
    nested: &LoadedMaps,
    kind_table: &BarrierKindTable,
    bridge_table: &BridgeKindTable,
) -> anyhow::Result<(MapLayout, MapConfig)> {
    let mut out = CompileOutput {
        layout: MapLayout::default(),
        config: MapConfig {
            grids: Vec::new(),
            actor_spawn_zones: Vec::new(),
            player_spawn_zones: Vec::new(),
            placed_items: Vec::new(),
            pressure_plates: Vec::new(),
        },
    };
    let scope = CompileScope {
        sizes,
        nested,
        kind_table,
        bridge_table,
    };
    compile_carrier(root, &scope, CarrierId::WORLD, &mut out)?;
    // The renderer indexes the material vectors by segment position, so any
    // length divergence is a bug here, not in the client.
    assert_eq!(out.layout.walls.len(), out.layout.wall_materials.len());
    assert_eq!(out.layout.floors.len(), out.layout.floor_materials.len());
    assert_eq!(out.layout.ramps.len(), out.layout.ramp_materials.len());
    Ok((out.layout, out.config))
}

struct CompileScope<'a> {
    sizes: MapGeometryConfig,
    nested: &'a LoadedMaps,
    kind_table: &'a BarrierKindTable,
    bridge_table: &'a BridgeKindTable,
}

struct CompileOutput {
    layout: MapLayout,
    config: MapConfig,
}

// One map's records onto `carrier`, then its nested maps as child
// carriers, each taking the next id right before it is compiled, so parents
// precede their descendants in the carrier list.
fn compile_carrier(
    map_def: &MapDef,
    scope: &CompileScope,
    carrier: CarrierId,
    out: &mut CompileOutput,
) -> anyhow::Result<()> {
    let cols = map_def.grid_cols;
    let rows = map_def.grid_rows;
    let geometry = MapGeometry::new(cols, rows, scope.sizes);
    let kind_table = scope.kind_table;
    let bridge_table = scope.bridge_table;
    // Face materials are authored per file, against its own grid.
    let assets = MaterialRules::from_def(map_def, scope.sizes);

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

    // Barriers controlled by a pressure plate are treated as always open for
    // actor pathfinding: actors can't open them, but they seal a room with no
    // alternative route, so assuming open lets a returning actor head home and
    // physics holds it at the barrier until someone opens it. Every other
    // barrier (key-only / static) stays closed for actors.
    let pressure_plates = pressure_plates(map_def, kind_table, bridge_table, carrier)?;
    let pressure_plate_kinds: HashSet<BarrierKindId> = pressure_plates
        .iter()
        .filter_map(|plate| match plate.purpose {
            PlatePurpose::Barrier(kind) => Some(kind),
            PlatePurpose::Bridge(_) | PlatePurpose::Firework => None,
        })
        .collect();

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
            // Barriers sit on grid edges just like walls; mark the ones actors
            // can never pass so pathfinding routes around them. Pressure-plate
            // barriers are skipped (treated as open — see `pressure_plate_kinds`).
            for barrier in &level.barriers {
                let opened_by_plate = kind_table
                    .resolve(&barrier.kind)
                    .is_ok_and(|kind| pressure_plate_kinds.contains(&kind));
                if !opened_by_plate {
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
        let (merged_walls, merged_materials) = walls::merge_walls(tier, &assets);
        all_walls.extend(merged_walls);
        all_wall_materials.extend(merged_materials);
    }

    let mut barrier_edges: Vec<Vec<BarrierEdge>> = Vec::with_capacity(map_def.levels.len());
    for (level_idx, level) in map_def.levels.iter().enumerate() {
        let mut edges = Vec::with_capacity(level.barriers.len());
        for (barrier_idx, b) in level.barriers.iter().enumerate() {
            let kind = kind_table
                .resolve(&b.kind)
                .with_context(|| format!("level {level_idx} barriers[{barrier_idx}]"))?;
            edges.push(BarrierEdge {
                edge: [b.c0, b.r0, b.c1, b.r1],
                kind,
            });
        }
        barrier_edges.push(edges);
    }
    let mut all_barriers = merge_barriers(stack_barriers(&barrier_edges, &slab_masks, &geometry));

    let mut all_light_bridges = light_bridges(map_def, &geometry, bridge_table)?;

    let mut all_floors: Vec<Floor> = Vec::new();
    let mut all_floor_materials: Vec<FaceMaterials> = Vec::new();
    for (level_idx, m) in slab_masks.iter().enumerate() {
        let level_u8 = u8::try_from(level_idx).unwrap_or(u8::MAX);
        let y = geometry.level_y(level_u8);
        // Tell floor emission to skip its corner-filler strip at the high
        // end of each z-axis ramp arriving at this level — a strip there
        // would hover above where the slope already meets the upper floor.
        let mut skip_corner_filler_edges: HashSet<(i32, i32)> = HashSet::new();
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
        let (merged_floors, merged_materials) = floors::merge_floors(tier, &assets);
        all_floors.extend(merged_floors);
        all_floor_materials.extend(merged_materials);
    }

    // Grass on floorless cells is silently dropped (like out-of-place wall
    // lights): the editor already enforces floor presence, and a hard error
    // would brick server startup over a cosmetic feature.
    let mut grass: Vec<GrassCell> = Vec::new();
    for (level_idx, level) in map_def.levels.iter().enumerate() {
        let level_u8 = u8::try_from(level_idx).unwrap_or(u8::MAX);
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

    let mut ramps_out = ramps::specs_to_ramps(&geometry, &ramp_specs);
    let ramp_materials: Vec<FaceMaterials> = ramps_out.iter().map(|r| assets.materials_for_ramp_top(r)).collect();

    let mut ladders: Vec<Ladder> = map_def
        .ladders
        .iter()
        .map(|def| ladder_from_def(def, &geometry))
        .collect();

    // Every record of this map is on its carrier, in this map's own frame.
    for wall in &mut all_walls {
        wall.carrier = carrier;
    }
    for floor in &mut all_floors {
        floor.carrier = carrier;
    }
    for ramp in &mut ramps_out {
        ramp.carrier = carrier;
    }
    for barrier in &mut all_barriers {
        barrier.carrier = carrier;
    }
    for bridge in &mut all_light_bridges {
        bridge.carrier = carrier;
    }
    for light in &mut wall_lights {
        light.carrier = carrier;
    }
    for ladder in &mut ladders {
        ladder.carrier = carrier;
    }

    let placed_items = placed_items(map_def, kind_table, &level_grids, carrier)?;

    let layout = &mut out.layout;
    layout.walls.extend(all_walls);
    layout.wall_materials.extend(all_wall_materials);
    layout.ramps.extend(ramps_out);
    layout.ramp_materials.extend(ramp_materials);
    layout.wall_lights.extend(wall_lights);
    layout.floors.extend(all_floors);
    layout.floor_materials.extend(all_floor_materials);
    layout.barriers.extend(all_barriers);
    layout.light_bridges.extend(all_light_bridges);
    layout
        .pressure_plates
        .extend(pressure_plates.iter().map(|p| PressurePlate {
            level: p.level,
            center_x: geometry.cell_center_x(p.col),
            center_y: geometry.level_y(p.level),
            center_z: geometry.cell_center_z(p.row),
            purpose: p.purpose,
            carrier,
        }));
    layout.ladders.extend(ladders);
    layout.grass.extend(grass);

    let config = &mut out.config;
    config.grids.push(CarrierGrid::new(carrier, geometry, level_grids));
    config.actor_spawn_zones.extend(actor_spawn_zones(map_def, carrier));
    config.player_spawn_zones.extend(player_spawn_zones(map_def, carrier));
    config.placed_items.extend(placed_items);
    config.pressure_plates.extend(pressure_plates);

    for entry in &map_def.nested_maps {
        let child_def = scope
            .nested
            .get(&entry.map)
            .expect("nested map missing from the loaded tree");
        let child_geometry = MapGeometry::new(child_def.grid_cols, child_def.grid_rows, scope.sizes);
        let id = next_carrier(out);
        out.layout
            .carriers
            .push(nested_carrier(&geometry, &child_geometry, &entry.motion, carrier));
        let reach = usize::from(out.layout.carrier_base_level(id))
            + child_def.levels.len()
            + usize::from(out.layout.carrier_motion_levels(id));
        assert!(
            reach <= usize::from(u8::MAX) + 1,
            "nested map {:?} reaches past the last storey a level tag can name",
            entry.map
        );
        compile_carrier(child_def, scope, id, out).with_context(|| format!("nested map {:?}", entry.map))?;
    }
    Ok(())
}

fn next_carrier(out: &CompileOutput) -> CarrierId {
    CarrierId(u16::try_from(out.layout.carriers.len() + 1).expect("more carriers than CarrierId can name"))
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

fn actor_spawn_zones(map_def: &MapDef, carrier: CarrierId) -> Vec<ActorSpawnZone> {
    map_def
        .actor_spawn_zones
        .iter()
        .map(|zone| ActorSpawnZone {
            carrier,
            level: u8::try_from(zone.level).unwrap_or(u8::MAX),
            cols: zone.cols,
            rows: zone.rows,
            kind: zone.kind.clone(),
            count: zone.count,
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

// Bridges deliberately set no `Cell` flags, so navigation, item cells,
// spawn cells, and the air graph never see them.
fn light_bridges(
    map_def: &MapDef,
    geometry: &MapGeometry,
    bridge_table: &BridgeKindTable,
) -> anyhow::Result<Vec<LightBridge>> {
    let mut out = Vec::new();
    for (level_idx, level) in map_def.levels.iter().enumerate() {
        let level_u8 = u8::try_from(level_idx).unwrap_or(u8::MAX);
        let cells = level
            .light_bridges
            .iter()
            .enumerate()
            .map(|(idx, def)| {
                let kind = bridge_table
                    .resolve(&def.kind)
                    .with_context(|| format!("level {level_idx} light_bridges[{idx}]"))?;
                Ok((def.col, def.row, kind))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        out.extend(merge_light_bridges(&cells).into_iter().map(|rect| LightBridge {
            x1: geometry.cell_to_world_x(rect.c0),
            z1: geometry.cell_to_world_z(rect.r0),
            x2: geometry.cell_to_world_x(rect.c1),
            z2: geometry.cell_to_world_z(rect.r1),
            y: geometry.level_y(level_u8),
            thickness: geometry.bridge_thickness(),
            level: level_u8,
            kind: rect.kind,
            carrier: CarrierId::WORLD,
        }));
    }
    Ok(out)
}

fn pressure_plates(
    map_def: &MapDef,
    kind_table: &BarrierKindTable,
    bridge_table: &BridgeKindTable,
    carrier: CarrierId,
) -> anyhow::Result<Vec<PressurePlateRuntime>> {
    map_def
        .pressure_plates
        .iter()
        .enumerate()
        .map(|(idx, p)| {
            let purpose = match &p.purpose {
                PressurePlatePurposeDef::Barrier { kind } => PlatePurpose::Barrier(
                    kind_table
                        .resolve(kind)
                        .with_context(|| format!("pressure_plates[{idx}]"))?,
                ),
                PressurePlatePurposeDef::Bridge { kind } => PlatePurpose::Bridge(
                    bridge_table
                        .resolve(kind)
                        .with_context(|| format!("pressure_plates[{idx}]"))?,
                ),
                PressurePlatePurposeDef::Firework => PlatePurpose::Firework,
            };
            Ok(PressurePlateRuntime {
                carrier,
                level: u8::try_from(p.level).unwrap_or(u8::MAX),
                col: p.col,
                row: p.row,
                purpose,
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

// A carrier's motion between two points of its parent's frame, each end
// displaced from its anchor by its nudge, x and z in wall widths and y in
// floor thicknesses (`nudge_scale` holds the three sizes). The
// timing is whole ticks so both sides place it exactly from the shared
// tick, and a stationary motion is a single tick that never leaves its
// start. A tile is a nested one-cell map, so nothing here is special about
// it.
fn carrier_from_motion(end1: Vec3, end2: Vec3, motion: &MotionDef, nudge_scale: Vec3, parent: CarrierId) -> Carrier {
    let level = u8::try_from(motion.level).unwrap_or(u8::MAX);
    let to_level = u8::try_from(motion.to_level()).unwrap_or(u8::MAX);
    let from = end1 + Vec3::from(motion.from_nudge) * nudge_scale;
    let to = end2 + Vec3::from(motion.to_nudge) * nudge_scale;
    Carrier {
        parent,
        level: level.min(to_level),
        levels: level.abs_diff(to_level),
        from: from.into(),
        to: to.into(),
        travel_ticks: ticks_from_secs(motion.travel_secs).max(1),
        pause_ticks: ticks_from_secs(motion.pause_secs),
        phase_ticks: ticks_from_secs(motion.phase_secs),
    }
}

// A nested map's carrier: its origin's offset in the parent's frame puts
// the nested cell (0, 0) on the parent's `from` cell at storey `level`, and
// likewise `to` at `to_level`. Both grids are centered on their own origin,
// which is why the nested corner is subtracted.
fn nested_carrier(parent: &MapGeometry, nested: &MapGeometry, motion: &MotionDef, parent_id: CarrierId) -> Carrier {
    let level = u8::try_from(motion.level).unwrap_or(u8::MAX);
    let to_level = u8::try_from(motion.to_level()).unwrap_or(u8::MAX);
    let end1 = nested_origin_offset(parent, nested, motion.from, level);
    let end2 = nested_origin_offset(parent, nested, motion.to, to_level);
    let nudge_scale = Vec3::new(
        parent.wall_thickness(),
        parent.floor_thickness(),
        parent.wall_thickness(),
    );
    carrier_from_motion(end1, end2, motion, nudge_scale, parent_id)
}

fn nested_origin_offset(parent: &MapGeometry, nested: &MapGeometry, cell: [i32; 2], level: u8) -> Vec3 {
    Vec3::new(
        parent.cell_to_world_x(cell[0]) - nested.cell_to_world_x(0),
        parent.level_y(level),
        parent.cell_to_world_z(cell[1]) - nested.cell_to_world_z(0),
    )
}

// Convert an editor-authored `(cell, side)` ladder into a `Ladder` in the
// map's frame: the anchor edge's span shrunk to `LADDER_WIDTH` centered on
// the edge midpoint, with the normal pointing across the edge away from the
// anchor cell (into the climb volume). Side conventions match `lights.rs`:
// North = -Z, South = +Z, West = -X, East = +X.
fn ladder_from_def(def: &LadderDef, geometry: &MapGeometry) -> Ladder {
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
        carrier: CarrierId::WORLD,
    }
}
