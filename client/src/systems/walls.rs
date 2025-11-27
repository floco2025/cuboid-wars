use bevy::prelude::*;

use crate::{resources::WallConfig, spawning::spawn_wall};

// Marker component for walls
#[derive(Component)]
pub struct WallMarker;

// System to spawn walls when WallConfig is available
pub fn spawn_walls_system(
    mut commands: Commands,
    wall_config: Option<Res<WallConfig>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    existing_walls: Query<&WallMarker>,
    mut spawned: Local<bool>,
) {
    // Only spawn once when WallConfig becomes available
    if *spawned || wall_config.is_none() {
        return;
    }

    // Check if walls were already spawned
    if existing_walls.iter().count() > 0 {
        *spawned = true;
        return;
    }

    let wall_config = wall_config.unwrap();
    
    info!("Spawning {} wall segments", wall_config.walls.len());
    
    for wall in &wall_config.walls {
        spawn_wall(&mut commands, &mut meshes, &mut materials, wall);
    }
    
    *spawned = true;
}
