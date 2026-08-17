use bevy::{ecs::system::SystemParam, prelude::*};
use common::{
    config::GameplayConfig,
    protocol::{BarrierKindTable, MapLayout, MissileMarker, Position},
};

use crate::{
    actors::ActorGhostMap,
    barriers::BarrierAssets,
    config::{AssetSet, ClientSettings},
    items::ItemAssets,
    missiles::MissileAssets,
    network::resources::RoundTripTime,
    players::LocalPlayerInfo,
    projectiles::ProjectileAssets,
    ui::{GameMessageFeed, PendingBanner, QuestLog, SeenPlayerIds},
    vfx::{ExplosionAssets, ExplosionRadii, ExplosionVfxBudget},
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

    // Correction vector toward where the server is *now*: the snapshot
    // position is ~half an RTT old, so project it forward before comparing
    // against the client position captured at insert.
    #[must_use]
    pub fn extrapolated_delta(&self) -> Vec3 {
        Vec3::from(self.server_pos) + self.server_velocity * self.rtt / 2.0 - Vec3::from(self.client_pos)
    }
}

// Pick the axis with the largest |value| from a 3-component delta. Used
// for the per-axis snap decision and its warning log, so the reader sees
// which axis tripped the threshold.
pub fn worst_axis_divergence(delta: Vec3) -> (&'static str, f32) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn recon(client_pos: Position, server_pos: Position, server_velocity: Vec3, rtt: f32) -> ServerReconciliation {
        ServerReconciliation {
            client_pos,
            server_pos,
            server_velocity,
            correction_progress: 0.0,
            rtt,
        }
    }

    fn pos(x: f32, y: f32, z: f32) -> Position {
        Position { x, y, z }
    }

    #[test]
    fn extrapolated_delta_without_velocity_is_the_plain_offset() {
        let recon = recon(pos(1.0, 0.0, 0.0), pos(3.0, 0.0, 0.0), Vec3::ZERO, 0.2);
        assert_eq!(recon.extrapolated_delta(), Vec3::new(2.0, 0.0, 0.0));
    }

    #[test]
    fn extrapolated_delta_projects_server_forward_by_half_rtt() {
        let recon = recon(pos(0.0, 0.0, 0.0), pos(0.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0), 0.2);
        assert_eq!(recon.extrapolated_delta(), Vec3::new(1.0, 0.0, 0.0));
    }

    #[test]
    fn divergence_picks_the_dominant_axis() {
        assert_eq!(worst_axis_divergence(Vec3::new(-3.0, 1.0, 2.0)), ("x", 3.0));
        assert_eq!(worst_axis_divergence(Vec3::new(1.0, -3.0, 2.0)), ("y", 3.0));
        assert_eq!(worst_axis_divergence(Vec3::new(1.0, 2.0, -3.0)), ("z", 3.0));
    }

    #[test]
    fn divergence_ties_prefer_x_then_y() {
        assert_eq!(worst_axis_divergence(Vec3::splat(2.0)), ("x", 2.0));
        assert_eq!(worst_axis_divergence(Vec3::new(0.0, 2.0, 2.0)), ("y", 2.0));
    }
}

#[derive(SystemParam)]
pub struct AssetManagers<'w> {
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub materials: ResMut<'w, Assets<StandardMaterial>>,
    pub images: ResMut<'w, Assets<Image>>,
    pub graphs: ResMut<'w, Assets<AnimationGraph>>,
}

// ============================================================================
// System Parameters
// ============================================================================

// Read-only asset handles and config the message handlers draw from.
#[derive(SystemParam)]
pub struct ClientAssetHandles<'w> {
    pub asset_server: Res<'w, AssetServer>,
    pub asset_set: Res<'w, AssetSet>,
    pub client_settings: Res<'w, ClientSettings>,
    pub projectile_assets: Res<'w, ProjectileAssets>,
    pub explosion_assets: Res<'w, ExplosionAssets>,
    pub explosion_radii: Res<'w, ExplosionRadii>,
    pub item_assets: Res<'w, ItemAssets>,
    pub barrier_assets: Res<'w, BarrierAssets>,
    pub missile_assets: Res<'w, MissileAssets>,
    pub gameplay_config: Res<'w, GameplayConfig>,
    pub barrier_kind_table: Res<'w, BarrierKindTable>,
}

// Mutable HUD-facing state the handlers write (feeds, banners, quest log,
// the local player's own flags).
#[derive(SystemParam)]
pub struct HudState<'w> {
    pub local_player_info: ResMut<'w, LocalPlayerInfo>,
    pub game_message_feed: ResMut<'w, GameMessageFeed>,
    pub seen_player_ids: ResMut<'w, SeenPlayerIds>,
    pub quest_log: ResMut<'w, QuestLog>,
    pub pending_banner: ResMut<'w, PendingBanner>,
}

// Mutable world-replication state driven by snapshots and cues.
#[derive(SystemParam)]
pub struct WorldSyncState<'w, 's> {
    pub explosion_vfx_budget: ResMut<'w, ExplosionVfxBudget>,
    pub map_layout: Option<Res<'w, MapLayout>>,
    pub open_barrier_kinds: ResMut<'w, crate::barriers::OpenBarrierKinds>,
    pub rain_intensity: ResMut<'w, crate::vfx::RainIntensity>,
    pub night_lighting: ResMut<'w, crate::map::skybox::NightLighting>,
    pub actor_ghosts: ResMut<'w, ActorGhostMap>,
    pub missile_map: ResMut<'w, crate::missiles::MissileMap>,
    pub missile_data: Query<'w, 's, &'static Position, With<MissileMarker>>,
    pub firework_show: ResMut<'w, crate::vfx::FireworkShow>,
}

// The one bundle threaded through message dispatch. Nested role bundles keep
// the enclosing system under Bevy's parameter limit while call sites say
// which role they touch.
#[derive(SystemParam)]
pub struct ClientAssets<'w, 's> {
    pub handles: ClientAssetHandles<'w>,
    pub hud: HudState<'w>,
    pub world_sync: WorldSyncState<'w, 's>,
}
