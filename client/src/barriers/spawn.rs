use bevy::prelude::*;

use super::BarrierAssets;
use crate::constants::BARRIER_OVERLAP_EPS;
use crate::map::MapLevel;
use common::{
    physics::OpenBarrierKinds,
    protocol::{Barrier, BarrierKindId, MapLayout},
};

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
    map_layout: Option<Res<MapLayout>>,
    barrier_assets: Option<Res<BarrierAssets>>,
    existing: Query<Entity, With<BarrierMarker>>,
) {
    let Some(layout) = map_layout else { return };
    let Some(barrier_assets) = barrier_assets else { return };
    if !layout.is_changed() {
        return;
    }

    for entity in &existing {
        commands.entity(entity).despawn();
    }

    for barrier in &layout.barriers {
        spawn_barrier(&mut commands, &barrier_assets, barrier);
    }
}

fn spawn_barrier(commands: &mut Commands, assets: &BarrierAssets, barrier: &Barrier) {
    use common::constants::{BARRIER_HEIGHT, LEVEL_HEIGHT};

    let center_x = f32::midpoint(barrier.x1, barrier.x2);
    let center_z = f32::midpoint(barrier.z1, barrier.z2);
    let dx = barrier.x2 - barrier.x1;
    let dz = barrier.z2 - barrier.z1;
    let length = dx.hypot(dz);
    let rotation = Quat::from_rotation_y(dz.atan2(dx));
    let center_y = f32::from(barrier.level) * LEVEL_HEIGHT + BARRIER_HEIGHT / 2.0;

    // Grow the segment by `BARRIER_OVERLAP_EPS` on each side along the long
    // axis (X local) and at the top/bottom (Y local), so coplanar contacts
    // with abutting walls and floor slabs win the depth test instead of
    // z-fighting. Thickness stays as baked in the mesh.
    let scale = Vec3::new(
        length + 2.0 * BARRIER_OVERLAP_EPS,
        BARRIER_HEIGHT + 2.0 * BARRIER_OVERLAP_EPS,
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
        Visibility::Visible,
    ));
}

// Force barriers of open kinds to `Visibility::Hidden`. Must run **after**
// `map_level_focus_visibility_system`, which also writes `Visibility` on
// barriers every tick (level focus toggles all map entities on the
// current level). Without the ordering the two systems race per frame →
// flicker.
//
// Why no `is_changed()` short-circuit: even if `OpenBarrierKinds` hasn't
// changed, level-focus may have just re-set every barrier on this level
// to Visible — we have to re-Hide the open ones each tick or they pop
// back. The barrier set is small (typically < 100 entities), so the
// unconditional loop is negligible.
//
// Only writes the `Hidden` direction; for closed kinds we leave whatever
// level-focus set, so a barrier on a different level stays hidden.
pub fn barriers_visibility_system(
    open: Res<OpenBarrierKinds>,
    mut barriers: Query<(&BarrierKindMarker, &mut Visibility), With<BarrierMarker>>,
) {
    for (kind, mut vis) in &mut barriers {
        if open.0.contains(&kind.0) {
            *vis = Visibility::Hidden;
        }
    }
}
