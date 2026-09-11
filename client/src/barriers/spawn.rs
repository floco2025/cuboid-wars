use bevy::prelude::*;

use super::BarrierAssets;
use crate::{
    carriers::{CarrierEntities, CarrierStoreys},
    fields::{FieldMeshes, VisualField, merge_fields, spawn_field_visual},
    map::{FocusedMapLevel, MapLevel, map_level_visibility},
};
use common::protocol::{BarrierId, MapLayout, MapSettings, PlateState};

#[derive(Component)]
pub struct BarrierMarker;

#[derive(Component)]
pub struct BarrierInstance(BarrierId);

pub fn barriers_spawn_system(
    mut commands: Commands,
    map_layout: Res<MapLayout>,
    settings: Res<MapSettings>,
    field_meshes: Res<FieldMeshes>,
    barrier_assets: Res<BarrierAssets>,
    plates: Res<PlateState>,
    focused: Res<FocusedMapLevel>,
    carrier_entities: Res<CarrierEntities>,
    storeys: Res<CarrierStoreys>,
    existing: Query<Entity, With<BarrierMarker>>,
) {
    let layout = map_layout;
    if !layout.is_changed() {
        return;
    }

    for entity in &existing {
        commands.entity(entity).despawn();
    }

    let fields = merge_fields(
        layout.barriers.iter().map(VisualField::from_barrier),
        &layout.floors,
        settings.geometry.floor_thickness,
    );
    for field in fields {
        let kind = field.kind.expect("barrier visual missing its kind");
        let level = storeys.tag(field.carrier, field.level, field.levels.saturating_sub(1));
        commands
            .spawn((
                BarrierMarker,
                BarrierInstance(field.barrier.expect("barrier visual missing its id")),
                level,
                ChildOf(carrier_entities.get(field.carrier)),
                field.transform(),
                barrier_visibility(
                    &plates.open_barriers,
                    *focused,
                    field.barrier.expect("barrier visual missing its id"),
                    level,
                ),
            ))
            .with_children(|parent| {
                spawn_field_visual(
                    parent,
                    &field_meshes,
                    &barrier_assets.kinds[kind.0 as usize],
                    &field,
                    &layout,
                );
            });
    }
}

pub fn barriers_visibility_system(
    plates: Res<PlateState>,
    focused: Res<FocusedMapLevel>,
    mut barriers: Query<(&BarrierInstance, &MapLevel, &mut Visibility), With<BarrierMarker>>,
) {
    if !plates.is_changed() && !focused.is_changed() {
        return;
    }
    // An input change affects only some barriers; equal writes would retrigger propagation on the rest.
    for (kind, level, mut visibility) in &mut barriers {
        visibility.set_if_neq(barrier_visibility(&plates.open_barriers, *focused, kind.0, *level));
    }
}

fn barrier_visibility(open: &[BarrierId], focused: FocusedMapLevel, kind: BarrierId, level: MapLevel) -> Visibility {
    if open.contains(&kind) {
        Visibility::Hidden
    } else {
        map_level_visibility(focused, level)
    }
}

#[cfg(test)]
#[path = "tests/spawn.rs"]
mod tests;
