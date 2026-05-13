use bevy::{ecs::system::SystemParam, prelude::*};
use common::{
    config::GameplayConfig,
    protocol::{BarrierKindTable, Position},
};

use crate::{
    barriers::BarrierAssets,
    config::{AssetSet, ClientSettings},
    items::ItemAssets,
    players::LocalPlayerInfo,
    projectiles::ProjectileAssets,
    ui::{GameMessageFeed, SeenPlayerIds},
};

// ============================================================================
// Components
// ============================================================================

// Server's authoritative snapshot for this entity used for reconciliation.
#[derive(Component)]
pub struct ServerReconciliation {
    pub client_pos: Position,
    pub server_pos: Position,
    pub server_velocity: Vec3,
    pub correction_progress: f32,
    pub rtt: f32,
}

// Pick the axis with the largest |value| from a 3-component delta. Used
// in snap-warning logs so the reader sees which axis tripped the
// per-axis snap threshold.
pub fn worst_axis_excess(delta: Vec3) -> (&'static str, f32) {
    let xa = delta.x.abs();
    let ya = delta.y.abs();
    let za = delta.z.abs();
    if xa >= ya && xa >= za {
        ("x", xa)
    } else if ya >= za {
        ("y", ya)
    } else {
        ("z", za)
    }
}

// ============================================================================
// System Parameters
// ============================================================================

// System params to reduce parameter count across message handlers.
#[derive(SystemParam)]
pub struct AssetManagers<'w> {
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub materials: ResMut<'w, Assets<StandardMaterial>>,
    pub images: ResMut<'w, Assets<Image>>,
    pub graphs: ResMut<'w, Assets<AnimationGraph>>,
}

#[derive(SystemParam)]
pub struct ClientAssets<'w> {
    pub asset_server: Res<'w, AssetServer>,
    pub asset_set: Res<'w, AssetSet>,
    pub client_settings: Res<'w, ClientSettings>,
    pub projectile_assets: Res<'w, ProjectileAssets>,
    pub item_assets: Res<'w, ItemAssets>,
    pub barrier_assets: Res<'w, BarrierAssets>,
    pub gameplay_config: Res<'w, GameplayConfig>,
    pub barrier_kind_table: Res<'w, BarrierKindTable>,
    pub local_player_info: ResMut<'w, LocalPlayerInfo>,
    pub game_message_feed: ResMut<'w, GameMessageFeed>,
    pub seen_player_ids: ResMut<'w, SeenPlayerIds>,
}
