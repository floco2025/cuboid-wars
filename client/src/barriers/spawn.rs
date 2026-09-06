use bevy::prelude::*;

use super::BarrierAssets;
use crate::carriers::{CarrierEntities, CarrierStoreys};
use crate::constants::BARRIER_OVERLAP_EPS;
use crate::map::{FocusedMapLevel, MapLevel, map_level_visibility};
use common::protocol::{Barrier, BarrierKindId, MapLayout, PlateState};

#[derive(Component)]
pub struct BarrierMarker;

// What the visibility rule needs beside `MapLevel`: the kind, hidden while
// pressure plates hold it open.
#[derive(Component)]
pub struct BarrierSpan {
    kind: BarrierKindId,
}

// Spawn one entity per `Barrier` in the current `MapLayout`. Re-runs whenever
// `MapLayout` is inserted or replaced (e.g., reconnect / map change).
pub fn barriers_spawn_system(
    mut commands: Commands,
    map_layout: Res<MapLayout>,
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

    for barrier in &layout.barriers {
        // `levels` counts the storeys spanned; the tag's span is how many
        // more than the first.
        let level = storeys.tag(barrier.carrier, barrier.level, barrier.levels.saturating_sub(1));
        spawn_barrier(
            &mut commands,
            &barrier_assets,
            carrier_entities.get(barrier.carrier),
            level,
            barrier,
            barrier_visibility(&plates.open_barrier_kinds, *focused, barrier.kind, level),
        );
    }
}

fn spawn_barrier(
    commands: &mut Commands,
    assets: &BarrierAssets,
    carrier: Entity,
    level: MapLevel,
    barrier: &Barrier,
    visibility: Visibility,
) {
    let center_x = f32::midpoint(barrier.x1, barrier.x2);
    let center_z = f32::midpoint(barrier.z1, barrier.z2);
    let dx = barrier.x2 - barrier.x1;
    let dz = barrier.z2 - barrier.z1;
    let length = dx.hypot(dz);
    let rotation = Quat::from_rotation_y(dz.atan2(dx));
    let center_y = barrier.y + barrier.height / 2.0;

    // Grow the segment by `BARRIER_OVERLAP_EPS` on each side along the long
    // axis (X local) and at the top/bottom (Y local), so coplanar contacts
    // with abutting walls and floor slabs win the depth test instead of
    // z-fighting. Thickness stays as baked in the mesh.
    let scale = Vec3::new(
        length + 2.0 * BARRIER_OVERLAP_EPS,
        barrier.height + 2.0 * BARRIER_OVERLAP_EPS,
        1.0,
    );

    commands.spawn((
        BarrierMarker,
        BarrierSpan { kind: barrier.kind },
        level,
        ChildOf(carrier),
        Mesh3d(assets.mesh.clone()),
        MeshMaterial3d(assets.material_for(barrier.kind).clone()),
        Transform {
            translation: Vec3::new(center_x, center_y, center_z),
            rotation,
            scale,
        },
        visibility,
    ));
}

pub fn barriers_visibility_system(
    plates: Res<PlateState>,
    focused: Res<FocusedMapLevel>,
    mut barriers: Query<(&BarrierSpan, &MapLevel, &mut Visibility), With<BarrierMarker>>,
) {
    if !plates.is_changed() && !focused.is_changed() {
        return;
    }
    // An input change affects only some barriers; equal writes would retrigger propagation on the rest.
    for (span, level, mut visibility) in &mut barriers {
        visibility.set_if_neq(barrier_visibility(
            &plates.open_barrier_kinds,
            *focused,
            span.kind,
            *level,
        ));
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
