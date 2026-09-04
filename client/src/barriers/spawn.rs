use bevy::prelude::*;

use super::BarrierAssets;
use crate::constants::BARRIER_OVERLAP_EPS;
use crate::map::{FocusedMapLevel, MapLevel};
use common::protocol::{Barrier, BarrierKindId, MapLayout, PlateState};

#[derive(Component)]
pub struct BarrierMarker;

// Tags the barrier entity with its kind so the visibility system can hide
// matching entities when the server reports the kind currently open via
// pressure plates.
#[derive(Component)]
pub struct BarrierKindMarker(pub BarrierKindId);

// Spawn one entity per `Barrier` in the current `MapLayout`. Re-runs whenever
// `MapLayout` is inserted or replaced (e.g., reconnect / map change).
pub fn barriers_spawn_system(
    mut commands: Commands,
    map_layout: Res<MapLayout>,
    barrier_assets: Res<BarrierAssets>,
    plates: Res<PlateState>,
    focused: Res<FocusedMapLevel>,
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
        spawn_barrier(
            &mut commands,
            &barrier_assets,
            barrier,
            barrier_visibility(&plates.open_barrier_kinds, *focused, barrier.kind, barrier.level),
        );
    }
}

fn spawn_barrier(commands: &mut Commands, assets: &BarrierAssets, barrier: &Barrier, visibility: Visibility) {
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
        BarrierKindMarker(barrier.kind),
        MapLevel(barrier.level),
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
    mut barriers: Query<(&BarrierKindMarker, &MapLevel, &mut Visibility), With<BarrierMarker>>,
) {
    if !plates.is_changed() && !focused.is_changed() {
        return;
    }
    // An input change affects only some barriers; equal writes would retrigger propagation on the rest.
    for (kind, level, mut visibility) in &mut barriers {
        visibility.set_if_neq(barrier_visibility(
            &plates.open_barrier_kinds,
            *focused,
            kind.0,
            level.0,
        ));
    }
}

fn barrier_visibility(open: &[BarrierKindId], focused: FocusedMapLevel, kind: BarrierKindId, level: u8) -> Visibility {
    if open.contains(&kind) || focused.0.is_some_and(|focused| focused != level) {
        Visibility::Hidden
    } else {
        Visibility::Visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_combines_open_kind_and_level_focus() {
        let kind = BarrierKindId(2);

        assert_eq!(
            barrier_visibility(&[kind], FocusedMapLevel(Some(1)), kind, 1),
            Visibility::Hidden
        );
        assert_eq!(
            barrier_visibility(&[], FocusedMapLevel(Some(2)), kind, 1),
            Visibility::Hidden
        );
        assert_eq!(
            barrier_visibility(&[], FocusedMapLevel(Some(1)), kind, 1),
            Visibility::Visible
        );
    }
}
