use std::{collections::HashSet, iter::once};

use anyhow::Context;
use bevy::math::Vec3;

use super::{
    geometry::compile_geometry,
    load::LoadedMaps,
    schema::{MapDef, MotionDef, PressurePlatePurposeDef},
};
use crate::map::MapConfig;
use common::{
    config::MapGeometryConfig,
    map::MapGeometry,
    protocol::{BarrierKindTable, BridgeKindTable, Carrier, CarrierId, MapLayout, ticks_from_secs},
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
        kind_table,
        bridge_table,
        plate_barrier_kinds: once(root)
            .chain(nested.values())
            .flat_map(|map| &map.pressure_plates)
            .filter_map(|plate| match &plate.purpose {
                PressurePlatePurposeDef::Barrier { kind } => Some(kind.as_str()),
                PressurePlatePurposeDef::Bridge { .. } | PressurePlatePurposeDef::Firework => None,
            })
            .collect(),
    };
    compile_tree(root, nested, &scope, CarrierId::WORLD, &mut out)?;
    // The renderer indexes the material vectors by segment position, so any
    // length divergence is a bug here, not in the client.
    assert_eq!(out.layout.walls.len(), out.layout.wall_materials.len());
    assert_eq!(out.layout.floors.len(), out.layout.floor_materials.len());
    assert_eq!(out.layout.ramps.len(), out.layout.ramp_materials.len());
    Ok((out.layout, out.config))
}

pub(super) struct CompileScope<'a> {
    pub(super) sizes: MapGeometryConfig,
    pub(super) kind_table: &'a BarrierKindTable,
    pub(super) bridge_table: &'a BridgeKindTable,
    // Plate effects span the whole tree; actors may plan through these barriers and wait for physics to let them pass.
    pub(super) plate_barrier_kinds: HashSet<&'a str>,
}

pub(super) struct CompileOutput {
    pub(super) layout: MapLayout,
    pub(super) config: MapConfig,
}

// Parents precede their descendants in the carrier list, so each child gets its id immediately before compilation.
fn compile_tree(
    map_def: &MapDef,
    nested: &LoadedMaps,
    scope: &CompileScope,
    carrier: CarrierId,
    out: &mut CompileOutput,
) -> anyhow::Result<()> {
    let geometry = MapGeometry::new(map_def.grid_cols, map_def.grid_rows, scope.sizes);
    compile_geometry(map_def, geometry, scope, carrier, out)?;

    for entry in &map_def.nested_maps {
        let child_def = nested.get(&entry.map).expect("nested map missing from the loaded tree");
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
        compile_tree(child_def, nested, scope, id, out).with_context(|| format!("nested map {:?}", entry.map))?;
    }
    Ok(())
}

fn next_carrier(out: &CompileOutput) -> CarrierId {
    CarrierId(u16::try_from(out.layout.carriers.len() + 1).expect("more carriers than CarrierId can name"))
}

// A carrier's motion between two points of its parent's frame, each end
// displaced from its anchor by its nudge, x and z in wall widths and y in
// floor thicknesses (`nudge_scale` holds the three sizes). The
// timing is whole ticks so both sides place it exactly from the shared
// tick, and a stationary motion never leaves its start.
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
