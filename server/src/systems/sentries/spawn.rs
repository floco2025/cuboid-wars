use bevy::prelude::*;
use rand::Rng as _;

use crate::{
    map::cell_center,
    resources::{GridConfig, SentryGrid, SentryInfo, SentryMap, SentryMode, SentrySpawnConfig},
};
use common::{constants::*, markers::SentryMarker, protocol::*};

// System to spawn initial sentries on server startup
pub fn sentries_spawn_system(
    mut commands: Commands,
    mut sentries: ResMut<SentryMap>,
    mut sentry_grid: ResMut<SentryGrid>,
    grid_config: Res<GridConfig>,
    spawn_config: Res<SentrySpawnConfig>,
    query: Query<&SentryId, With<SentryMarker>>,
) {
    // Only spawn if no sentries exist yet
    if !query.is_empty() {
        return;
    }

    let mut rng = rand::rng();

    for i in 0..spawn_config.num_sentries {
        // Pick a random grid cell that doesn't have a sentry or a ramp
        let (grid_x, grid_z) = loop {
            let grid_x = rng.random_range(0..GRID_COLS);
            let grid_z = rng.random_range(0..GRID_ROWS);

            if sentry_grid.0[grid_z as usize][grid_x as usize].is_some() {
                continue;
            }

            if grid_config.grid[grid_z as usize][grid_x as usize].has_ramp {
                continue;
            }

            break (grid_x, grid_z);
        };

        // Spawn at grid center
        let pos = cell_center(grid_x, grid_z);

        // Start with zero velocity - patrol movement will pick initial direction
        let vel = Velocity { x: 0.0, y: 0.0, z: 0.0 };
        let face_dir = 0.0;

        let sentry_id = SentryId(i);
        let entity = commands
            .spawn((SentryMarker, sentry_id, pos, vel, FaceDirection(face_dir)))
            .id();

        sentries.0.insert(
            sentry_id,
            SentryInfo {
                entity,
                mode: SentryMode::Patrol,
                mode_timer: 0.0,
                follow_target: None,
                at_intersection: true,
            },
        );

        // Add to field map (only current cell, no heading yet since velocity is zero)
        sentry_grid.0[grid_z as usize][grid_x as usize] = Some(sentry_id);
    }
}
