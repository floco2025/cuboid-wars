use bevy::prelude::*;

use super::BarrierAssets;
use crate::{
    carriers::{CarrierEntities, CarrierStoreys},
    map::{FocusedMapLevel, MapLevel, map_level_visibility},
};
use common::protocol::{Barrier, BarrierKindId, MapLayout, PlateState};

#[derive(Component)]
pub struct BarrierMarker;

#[derive(Component)]
pub struct BarrierKind(BarrierKindId);

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
    commands.spawn((
        BarrierMarker,
        BarrierKind(barrier.kind),
        level,
        ChildOf(carrier),
        Mesh3d(assets.mesh.clone()),
        MeshMaterial3d(assets.material_for(barrier.kind).clone()),
        barrier_transform(barrier),
        visibility,
    ));
}

fn barrier_transform(barrier: &Barrier) -> Transform {
    let dx = barrier.x2 - barrier.x1;
    let dz = barrier.z2 - barrier.z1;
    Transform {
        translation: Vec3::new(
            f32::midpoint(barrier.x1, barrier.x2),
            barrier.y + barrier.height / 2.0,
            f32::midpoint(barrier.z1, barrier.z2),
        ),
        rotation: Quat::from_rotation_y(-dz.atan2(dx)),
        scale: Vec3::new(dx.hypot(dz), barrier.height, 1.0),
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
    use common::protocol::CarrierId;

    #[test]
    fn rectangle_matches_segment_endpoints_and_full_height_without_overlap() {
        for (dx, dz) in [(8.0, 0.0), (0.0, 8.0), (-8.0, 0.0), (0.0, -8.0)] {
            let barrier = Barrier {
                x1: 2.0,
                z1: 3.0,
                x2: 2.0 + dx,
                z2: 3.0 + dz,
                y: 4.0,
                height: 12.0,
                width: 0.1,
                level: 1,
                levels: 3,
                kind: BarrierKindId(0),
                carrier: CarrierId(1),
            };
            let transform = barrier_transform(&barrier);
            for (local_y, y) in [(-0.5, barrier.y), (0.5, barrier.y + barrier.height)] {
                let start = transform.transform_point(Vec3::new(-0.5, local_y, 0.0));
                let end = transform.transform_point(Vec3::new(0.5, local_y, 0.0));
                assert!(start.abs_diff_eq(Vec3::new(barrier.x1, y, barrier.z1), 1e-5));
                assert!(end.abs_diff_eq(Vec3::new(barrier.x2, y, barrier.z2), 1e-5));
            }
        }
    }

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
