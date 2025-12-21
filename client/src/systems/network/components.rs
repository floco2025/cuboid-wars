use bevy::{ecs::system::SystemParam, prelude::*};
use common::protocol::{Position, Velocity};

// ============================================================================
// Components
// ============================================================================

/// Server's authoritative snapshot for this entity used for reconciliation.
#[derive(Component)]
pub struct ServerReconciliation {
    pub client_pos: Position,
    pub server_pos: Position,
    pub server_vel: Velocity,
    pub timer: f32,
    pub rtt: f32,
}

// ============================================================================
// System Parameters
// ============================================================================

/// System params to reduce parameter count across message handlers.
#[derive(SystemParam)]
pub struct AssetManagers<'w> {
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub materials: ResMut<'w, Assets<StandardMaterial>>,
    pub images: ResMut<'w, Assets<Image>>,
    pub graphs: ResMut<'w, Assets<AnimationGraph>>,
}
