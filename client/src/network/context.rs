use bevy::{ecs::system::SystemParam, prelude::*};
use common::{
    config::{GameplayConfig, NetworkConfig},
    map::Carriers,
    physics::{CollisionWorld, PortalSet},
    protocol::*,
};

use crate::{
    actors::{ActorGhostMap, ActorMap},
    barriers::{BarrierAssets, LockedPlatePurposes},
    cameras::MainCameraMarker,
    carriers::{CarrierEntities, CarrierStoreys},
    characters::MaxHealth,
    config::{AssetSet, ClientSettings},
    input::PendingWeaponSelection,
    items::{ItemAssets, ItemMap},
    map::skybox::LightingState,
    missiles::{MissileAssets, MissileMap},
    network::{LastPlayerMovesTick, LastSnapshotTick, RoundTripTime, TickSync},
    players::{LocalPlayerInfo, MyPlayerId, PlayerMap},
    portals::{PortalAssets, PortalMap},
    projectiles::ProjectileAssets,
    ui::{HudBanner, MessageFeed, QuestLog},
    vfx::{
        BlastRadii, ExplosionAssets, ExplosionSpawnCtx, ExplosionVfxBudget, FireworkShow, PortalFizzleAssets,
        RainIntensity,
    },
};

// The read-only asset handles and per-kind tuning the handlers spawn and
// play cues from.
#[derive(SystemParam)]
pub(super) struct PresentationAssets<'w> {
    pub(super) asset_server: Res<'w, AssetServer>,
    pub(super) asset_set: Res<'w, AssetSet>,
    pub(super) item_assets: Res<'w, ItemAssets>,
    pub(super) barrier_assets: Res<'w, BarrierAssets>,
    pub(super) missile_assets: Res<'w, MissileAssets>,
    pub(super) portal_fizzle_assets: Res<'w, PortalFizzleAssets>,
    pub(super) portal_assets: Res<'w, PortalAssets>,
    pub(super) projectile_assets: Res<'w, ProjectileAssets>,
    pub(super) explosion_assets: Res<'w, ExplosionAssets>,
    pub(super) blast_radii: Res<'w, BlastRadii>,
    pub(super) max_health: Res<'w, MaxHealth>,
}

// Snapshot/player-movement ordering and the shared clock; actor samples order themselves in their buffers.
#[derive(SystemParam)]
pub(super) struct StreamClocks<'w> {
    pub(super) last_snapshot_tick: ResMut<'w, LastSnapshotTick>,
    pub(super) last_player_moves_tick: ResMut<'w, LastPlayerMovesTick>,
    pub(super) server_tick: ResMut<'w, ServerTick>,
    pub(super) tick_sync: ResMut<'w, TickSync>,
}

// Each resource appears once and the queries are read-only, so this needs no `ParamSet`.
#[derive(SystemParam)]
pub(super) struct ServerMessageContext<'w, 's> {
    pub(super) my_player_id: Res<'w, MyPlayerId>,
    pub(super) time: Res<'w, Time>,
    pub(super) network: Res<'w, NetworkConfig>,
    pub(super) rtt: ResMut<'w, RoundTripTime>,
    pub(super) assets: PresentationAssets<'w>,
    pub(super) clocks: StreamClocks<'w>,
    pub(super) client_settings: Res<'w, ClientSettings>,
    pub(super) gameplay_config: Res<'w, GameplayConfig>,
    pub(super) collision_world: Res<'w, CollisionWorld>,
    pub(super) carriers: Res<'w, Carriers>,
    pub(super) carrier_entities: Res<'w, CarrierEntities>,
    pub(super) carrier_storeys: Res<'w, CarrierStoreys>,
    pub(super) map_layout: Res<'w, MapLayout>,
    pub(super) map_settings: Res<'w, MapSettings>,
    pub(super) meshes: ResMut<'w, Assets<Mesh>>,
    pub(super) materials: ResMut<'w, Assets<StandardMaterial>>,
    pub(super) images: ResMut<'w, Assets<Image>>,
    pub(super) explosion_vfx_budget: ResMut<'w, ExplosionVfxBudget>,
    pub(super) players: ResMut<'w, PlayerMap>,
    pub(super) actors: ResMut<'w, ActorMap>,
    pub(super) items: ResMut<'w, ItemMap>,
    pub(super) missiles: ResMut<'w, MissileMap>,
    pub(super) portals: ResMut<'w, PortalMap>,
    pub(super) portal_set: ResMut<'w, PortalSet>,
    pub(super) portal_access: ResMut<'w, PortalAccess>,
    pub(super) pending_weapon_selection: ResMut<'w, PendingWeaponSelection>,
    pub(super) actor_ghosts: ResMut<'w, ActorGhostMap>,

    pub(super) local_player_info: ResMut<'w, LocalPlayerInfo>,
    pub(super) quest_log: ResMut<'w, QuestLog>,
    pub(super) banner: ResMut<'w, HudBanner>,
    pub(super) feed: ResMut<'w, MessageFeed>,
    pub(super) firework_show: ResMut<'w, FireworkShow>,
    pub(super) plates: ResMut<'w, PlateState>,
    pub(super) locked_plate_purposes: ResMut<'w, LockedPlatePurposes>,
    pub(super) rain_intensity: ResMut<'w, RainIntensity>,
    pub(super) lighting: ResMut<'w, LightingState>,
    pub(super) player_data: Query<'w, 's, &'static Position, With<PlayerMarker>>,
    pub(super) actor_data: Query<'w, 's, &'static Position, With<ActorMarker>>,
    pub(super) cameras: Query<'w, 's, Entity, (With<Camera3d>, With<MainCameraMarker>)>,
}

impl ServerMessageContext<'_, '_> {
    pub(super) fn explosion_ctx(&mut self) -> ExplosionSpawnCtx<'_> {
        ExplosionSpawnCtx {
            meshes: &mut self.meshes,
            materials: &mut self.materials,
            budget: &mut self.explosion_vfx_budget,
            explosion_assets: &self.assets.explosion_assets,
            gameplay_config: &self.gameplay_config,
            collision_world: Some(&self.collision_world),
            map_layout: Some(&self.map_layout),
            carriers: &self.carriers,
            carrier_entities: &self.carrier_entities,
            blast_radii: &self.assets.blast_radii,
        }
    }
}
