use bevy::{ecs::system::SystemParam, prelude::*};
use common::{
    config::GameplayConfig,
    protocol::{BarrierKindTable, Position},
};

use crate::{
    actors::ActorGhostMap,
    barriers::BarrierAssets,
    config::{AssetSet, ClientSettings},
    items::ItemAssets,
    network::resources::RoundTripTime,
    players::LocalPlayerInfo,
    projectiles::ProjectileAssets,
    ui::{GameMessageFeed, HudBannerMarker, QuestLog, SeenPlayerIds},
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

impl ServerReconciliation {
    // Fresh reconciliation target: zero correction progress, RTT captured as
    // seconds (centralizing the `Duration → f32` conversion).
    #[must_use]
    pub fn new(client_pos: Position, server_pos: Position, server_velocity: Vec3, rtt: &RoundTripTime) -> Self {
        Self {
            client_pos,
            server_pos,
            server_velocity,
            correction_progress: 0.0,
            rtt: rtt.rtt.as_secs_f32(),
        }
    }
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
pub struct ClientAssets<'w, 's> {
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
    pub quest_log: ResMut<'w, QuestLog>,
    pub open_barrier_kinds: ResMut<'w, crate::barriers::OpenBarrierKinds>,
    pub actor_ghosts: ResMut<'w, ActorGhostMap>,
    // Live banner entities — used by `spawn_hud_banner` to enforce the
    // single-banner invariant (despawn any existing before inserting the
    // new one). Bundled here so `dispatch_message` stays under Bevy's
    // 16-param system tuple limit.
    pub banners: Query<'w, 's, Entity, With<HudBannerMarker>>,
}
