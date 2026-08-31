use bevy::{ecs::system::SystemParam, prelude::*};
use common::{
    config::GameplayConfig,
    physics::{CollisionWorld, PortalSet},
    protocol::*,
};

use crate::{
    actors::{ActorGhostMap, ActorMap},
    barriers::{BarrierAssets, LockedPlatePurposes, OpenBarrierKinds},
    cameras::MainCameraMarker,
    characters::MaxHealth,
    config::{AssetSet, ClientSettings},
    items::{ItemAssets, ItemMap},
    map::skybox::LightingState,
    missiles::{MissileAssets, MissileMap},
    network::{LastSnapshotSeq, RoundTripTime},
    players::{LocalPlayerInfo, MyPlayerId, PlayerMap},
    portals::{PortalAssets, PortalMap as PortalVisuals},
    projectiles::ProjectileAssets,
    ui::{HudBanner, MessageFeed, QuestLog},
    vfx::{BlastRadii, ExplosionAssets, ExplosionSpawnCtx, ExplosionVfxBudget, FireworkShow, RainIntensity},
};

// Each resource appears once and the queries are read-only, so this needs no `ParamSet`.
#[derive(SystemParam)]
pub(super) struct ServerMessageContext<'w, 's> {
    pub(super) barrier_kind_table: Res<'w, BarrierKindTable>,
    pub(super) my_player_id: Option<Res<'w, MyPlayerId>>,
    pub(super) time: Res<'w, Time>,
    pub(super) rtt: ResMut<'w, RoundTripTime>,
    pub(super) asset_server: Res<'w, AssetServer>,
    pub(super) asset_set: Res<'w, AssetSet>,
    pub(super) client_settings: Res<'w, ClientSettings>,
    pub(super) gameplay_config: Res<'w, GameplayConfig>,
    pub(super) collision_world: Option<Res<'w, CollisionWorld>>,
    pub(super) map_layout: Option<Res<'w, MapLayout>>,
    pub(super) meshes: ResMut<'w, Assets<Mesh>>,
    pub(super) materials: ResMut<'w, Assets<StandardMaterial>>,
    pub(super) images: ResMut<'w, Assets<Image>>,
    pub(super) graphs: ResMut<'w, Assets<AnimationGraph>>,
    pub(super) explosion_vfx_budget: ResMut<'w, ExplosionVfxBudget>,
    pub(super) explosion_assets: Res<'w, ExplosionAssets>,
    pub(super) blast_radii: Res<'w, BlastRadii>,
    pub(super) max_health: Res<'w, MaxHealth>,
    pub(super) item_assets: Res<'w, ItemAssets>,
    pub(super) barrier_assets: Res<'w, BarrierAssets>,
    pub(super) missile_assets: Res<'w, MissileAssets>,
    pub(super) portal_assets: Res<'w, PortalAssets>,
    pub(super) projectile_assets: Res<'w, ProjectileAssets>,
    pub(super) players: ResMut<'w, PlayerMap>,
    pub(super) actors: ResMut<'w, ActorMap>,
    pub(super) items: ResMut<'w, ItemMap>,
    pub(super) missiles: ResMut<'w, MissileMap>,
    pub(super) portals: ResMut<'w, PortalVisuals>,
    pub(super) portal_set: ResMut<'w, PortalSet>,
    pub(super) actor_ghosts: ResMut<'w, ActorGhostMap>,
    pub(super) last_snapshot_seq: ResMut<'w, LastSnapshotSeq>,
    pub(super) local_player_info: ResMut<'w, LocalPlayerInfo>,
    pub(super) quest_log: ResMut<'w, QuestLog>,
    pub(super) banner: ResMut<'w, HudBanner>,
    pub(super) feed: ResMut<'w, MessageFeed>,
    pub(super) firework_show: ResMut<'w, FireworkShow>,
    pub(super) open_barrier_kinds: ResMut<'w, OpenBarrierKinds>,
    pub(super) locked_plate_purposes: ResMut<'w, LockedPlatePurposes>,
    pub(super) rain_intensity: ResMut<'w, RainIntensity>,
    pub(super) lighting: ResMut<'w, LightingState>,
    pub(super) player_data:
        Query<'w, 's, (&'static Position, &'static PlayerMoveIntent, &'static FaceYaw), With<PlayerMarker>>,
    pub(super) actor_data:
        Query<'w, 's, (&'static Position, &'static ActorMoveIntent, &'static FaceYaw), With<ActorMarker>>,
    pub(super) missile_data: Query<'w, 's, &'static Position, With<MissileMarker>>,
    pub(super) cameras: Query<'w, 's, Entity, (With<Camera3d>, With<MainCameraMarker>)>,
}

impl ServerMessageContext<'_, '_> {
    pub(super) fn explosion_ctx(&mut self) -> ExplosionSpawnCtx<'_> {
        ExplosionSpawnCtx {
            meshes: &mut self.meshes,
            materials: &mut self.materials,
            budget: &mut self.explosion_vfx_budget,
            explosion_assets: &self.explosion_assets,
            gameplay_config: &self.gameplay_config,
            collision_world: self.collision_world.as_deref(),
            map_layout: self.map_layout.as_deref(),
            blast_radii: &self.blast_radii,
        }
    }
}
