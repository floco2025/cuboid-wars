use bevy::prelude::*;

use super::BarrierAssets;
use crate::{
    carriers::{CarrierEntities, CarrierStoreys},
    fields::{FieldMeshes, VisualField, merge_fields, spawn_field_visual},
    map::{FocusedMapLevel, MapLevel, map_level_visibility},
};
use common::protocol::{BarrierKindId, MapLayout, MapSettings, PlateState};

#[derive(Component)]
pub struct BarrierMarker;

#[derive(Component)]
pub struct BarrierKind(BarrierKindId);

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
                BarrierKind(kind),
                level,
                ChildOf(carrier_entities.get(field.carrier)),
                field.transform(),
                barrier_visibility(&plates.open_barrier_kinds, *focused, kind, level),
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
    mut barriers: Query<(&BarrierKind, &MapLevel, &mut Visibility), With<BarrierMarker>>,
) {
    if !plates.is_changed() && !focused.is_changed() {
        return;
    }
    // An input change affects only some barriers; equal writes would retrigger propagation on the rest.
    for (kind, level, mut visibility) in &mut barriers {
        visibility.set_if_neq(barrier_visibility(&plates.open_barrier_kinds, *focused, kind.0, *level));
    }
}

fn barrier_visibility(
    open: &[BarrierKindId],
    focused: FocusedMapLevel,
    kind: BarrierKindId,
    level: MapLevel,
) -> Visibility {
    if open.contains(&kind) {
        Visibility::Hidden
    } else {
        map_level_visibility(focused, level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn level(level: u8, span: u8) -> MapLevel {
        MapLevel { level, span }
    }

    #[test]
    fn visibility_combines_open_kind_and_level_focus() {
        let kind = BarrierKindId(2);

        assert_eq!(
            barrier_visibility(&[kind], FocusedMapLevel(Some(1)), kind, level(1, 0)),
            Visibility::Hidden
        );
        assert_eq!(
            barrier_visibility(&[], FocusedMapLevel(Some(2)), kind, level(1, 0)),
            Visibility::Hidden
        );
        assert_eq!(
            barrier_visibility(&[], FocusedMapLevel(Some(1)), kind, level(1, 0)),
            Visibility::Visible
        );
    }

    #[test]
    fn a_stacked_barrier_shows_on_every_storey_it_spans() {
        let kind = BarrierKindId(0);

        assert_eq!(
            barrier_visibility(&[], FocusedMapLevel(Some(2)), kind, level(1, 1)),
            Visibility::Visible
        );
        assert_eq!(
            barrier_visibility(&[], FocusedMapLevel(Some(3)), kind, level(1, 1)),
            Visibility::Hidden
        );
        assert_eq!(
            barrier_visibility(&[], FocusedMapLevel(Some(0)), kind, level(1, 1)),
            Visibility::Hidden
        );
    }
}
