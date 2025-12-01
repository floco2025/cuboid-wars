use bevy::prelude::*;

use crate::{resources::WallConfig, spawning::{spawn_roof, spawn_wall}};

// Marker component for walls
#[derive(Component)]
pub struct WallMarker;

// Marker component for roofs
#[derive(Component)]
pub struct RoofMarker;

// System to spawn walls and roofs when WallConfig is available
pub fn spawn_walls_system(
    mut commands: Commands,
    wall_config: Option<Res<WallConfig>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut spawned: Local<bool>,
) {
    // Spawn exactly once after the server shares its wall configuration
    let Some(wall_config) = wall_config else {
        return;
    };

    if *spawned {
        return;
    }

    info!("Spawning {} wall segments and {} roofs", wall_config.walls.len(), wall_config.roofs.len());

    for wall in &wall_config.walls {
        spawn_wall(&mut commands, &mut meshes, &mut materials, wall);
    }

    for roof in &wall_config.roofs {
        spawn_roof(&mut commands, &mut meshes, &mut materials, roof);
    }

    *spawned = true;
}
