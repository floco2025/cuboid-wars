use bevy::{ecs::system::SystemParam, prelude::*};

use crate::{
    audio::{play_explosion_sound, play_sound},
    cameras::MainCameraMarker,
    characters::PreviousTickPosition,
    config::{AssetSet, ClientSettings},
    network::{RoundTripTime, ServerReconciliation},
    players::{CameraShake, CuboidShake, LocalPlayerInfo, PlayerMap},
    projectiles::{ProjectileAssets, spawn_projectiles},
    ui::{BannerMessage, HudBanner},
    vfx::{BlastRadii, ExplosionAssets, ExplosionSpawnCtx, ExplosionVfxBudget, spawn_player_explosion},
};
use common::{
    config::GameplayConfig,
    physics::{
        CharacterVerticalVelocity, CollisionWorld, KnockbackVelocity, OpenBarrierKinds, player_control_velocity,
    },
    protocol::*,
};

#[derive(SystemParam)]
pub(in crate::network) struct PlayerMessageContext<'w, 's> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    explosion_vfx_budget: ResMut<'w, ExplosionVfxBudget>,
    explosion_assets: Res<'w, ExplosionAssets>,
    gameplay_config: Res<'w, GameplayConfig>,
    collision_world: Option<Res<'w, CollisionWorld>>,
    map_layout: Option<Res<'w, MapLayout>>,
    asset_server: Res<'w, AssetServer>,
    asset_set: Res<'w, AssetSet>,
    client_settings: Res<'w, ClientSettings>,
    blast_radii: Res<'w, BlastRadii>,
    players: ResMut<'w, PlayerMap>,
    local_player_info: ResMut<'w, LocalPlayerInfo>,
    banner: ResMut<'w, HudBanner>,
    projectile_assets: Res<'w, ProjectileAssets>,
    player_data: Query<'w, 's, (&'static Position, &'static PlayerMoveIntent, &'static FaceYaw), With<PlayerMarker>>,
    open_barrier_kinds: Res<'w, OpenBarrierKinds>,
    cameras: Query<'w, 's, Entity, (With<Camera3d>, With<MainCameraMarker>)>,
}

pub(in crate::network) fn handle_player_move_message(
    message: &SPlayerMove,
    commands: &mut Commands,
    rtt: &RoundTripTime,
    context: &mut PlayerMessageContext,
) {
    apply_player_move(
        commands,
        &context.players,
        &context.player_data,
        rtt,
        &context.gameplay_config,
        message,
    );
}

pub(in crate::network) fn handle_player_jump_message(
    message: &SPlayerJump,
    commands: &mut Commands,
    rtt: &RoundTripTime,
    context: &mut PlayerMessageContext,
) {
    apply_player_jump(
        commands,
        &context.players,
        &context.player_data,
        rtt,
        &context.gameplay_config,
        message,
    );
}

pub(in crate::network) fn handle_player_shot_message(
    message: &SPlayerShot,
    commands: &mut Commands,
    context: &mut PlayerMessageContext,
) {
    apply_player_shot(
        commands,
        &context.projectile_assets,
        &context.players,
        &context.player_data,
        message,
        context.collision_world.as_deref(),
        &context.gameplay_config,
        &context.open_barrier_kinds,
    );
}

pub(in crate::network) fn handle_player_death_message(
    message: &SPlayerDeath,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut PlayerMessageContext,
) {
    // Keep audio outside the state handler so its unit test does not need an asset server.
    play_explosion_sound(
        commands,
        &context.asset_server,
        context.asset_set.player_sound("explodes"),
        &context.client_settings.audio,
        Vec3::from(message.pos),
        Some(context.blast_radii.player),
    );
    let mut ctx = ExplosionSpawnCtx {
        meshes: &mut context.meshes,
        materials: &mut context.materials,
        budget: &mut context.explosion_vfx_budget,
        explosion_assets: &context.explosion_assets,
        gameplay_config: &context.gameplay_config,
        collision_world: context.collision_world.as_deref(),
        map_layout: context.map_layout.as_deref(),
    };
    apply_player_death(
        commands,
        &mut ctx,
        &context.blast_radii,
        &mut context.players,
        &mut context.local_player_info,
        &mut context.banner,
        my_player_id,
        message,
    );
}

pub(in crate::network) fn handle_player_hit_message(
    message: &SPlayerHit,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut PlayerMessageContext,
) {
    apply_player_hit(
        commands,
        &context.players,
        &context.cameras,
        &context.client_settings,
        my_player_id,
        message,
    );
}

pub(in crate::network) fn handle_player_fall_damage_message(
    message: &SPlayerFallDamage,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut PlayerMessageContext,
) {
    apply_player_fall_damage(
        commands,
        &context.players,
        &context.cameras,
        &context.client_settings,
        my_player_id,
        &context.asset_server,
        &context.asset_set,
        message,
    );
}

pub(in crate::network) fn handle_player_blast_message(
    message: &SPlayerBlast,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut PlayerMessageContext,
) {
    apply_player_blast(commands, &context.players, my_player_id, message);
}

pub(in crate::network) fn handle_player_status_message(
    message: &SPlayerStatus,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut PlayerMessageContext,
) {
    apply_player_status(
        commands,
        &mut context.players,
        message,
        my_player_id,
        &context.asset_server,
        &context.asset_set,
    );
}

pub(super) fn player_movement_velocity(
    movement: PlayerMovementState,
    gameplay_config: &GameplayConfig,
    has_speed_power_up: bool,
    movement_disabled: bool,
) -> Vec3 {
    let mut velocity = player_control_velocity(
        movement.move_intent,
        gameplay_config,
        has_speed_power_up,
        movement_disabled,
    );
    velocity.y = movement.vertical_velocity;
    velocity
}

// Handle player move update (intent + facing) with server reconciliation.
fn apply_player_move(
    commands: &mut Commands,
    players: &PlayerMap,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceYaw), With<PlayerMarker>>,
    rtt: &RoundTripTime,
    gameplay_config: &GameplayConfig,
    event: &SPlayerMove,
) {
    trace!("{:?} move: {:?}", event.id, event);
    if let Some(player) = players.get(&event.id) {
        let server_velocity = player_movement_velocity(
            event.movement,
            gameplay_config,
            player.power_up(PowerUpKind::Speed),
            player.stunned,
        );

        // Never the local player, so we can always overwrite intent + facing.
        let input = (event.movement.move_intent, FaceYaw(event.movement.face_yaw));
        // Add server reconciliation if we have client position
        if let Ok((client_pos, _, _)) = player_data.get(player.entity) {
            commands.entity(player.entity).insert((
                input,
                ServerReconciliation::new(*client_pos, event.movement.pos, server_velocity, rtt),
            ));
        } else {
            commands.entity(player.entity).insert(input);
        }
    }
}

fn apply_player_jump(
    commands: &mut Commands,
    players: &PlayerMap,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceYaw), With<PlayerMarker>>,
    rtt: &RoundTripTime,
    gameplay_config: &GameplayConfig,
    event: &SPlayerJump,
) {
    if let Some(player) = players.get(&event.id)
        && let Ok((client_pos, _, _)) = player_data.get(player.entity)
    {
        let server_velocity = player_movement_velocity(
            event.movement,
            gameplay_config,
            player.power_up(PowerUpKind::Speed),
            player.stunned,
        );
        commands.entity(player.entity).insert((
            event.movement.move_intent,
            FaceYaw(event.movement.face_yaw),
            CharacterVerticalVelocity(event.movement.vertical_velocity),
            ServerReconciliation::new(*client_pos, event.movement.pos, server_velocity, rtt),
        ));
    }
}

// Handle player shooting - spawn projectile(s) on client.
fn apply_player_shot(
    commands: &mut Commands,
    projectile_assets: &ProjectileAssets,
    players: &PlayerMap,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceYaw), With<PlayerMarker>>,
    event: &SPlayerShot,
    collision_world: Option<&CollisionWorld>,
    gameplay_config: &GameplayConfig,
    open_barrier_kinds: &OpenBarrierKinds,
) {
    trace!("{:?} shot: {:?}", event.id, event);
    if let Some(player) = players.get(&event.id) {
        commands.entity(player.entity).insert(FaceYaw(event.face_yaw));

        // Spawn projectile(s) based on player's multi-shot power-up status
        if let Ok((position, _, _)) = player_data.get(player.entity)
            && let Some(collision_world) = collision_world
        {
            spawn_projectiles(
                commands,
                projectile_assets,
                position,
                event.face_yaw,
                event.face_pitch,
                player.power_up(PowerUpKind::MultiShot),
                gameplay_config.player.eye_height(),
                gameplay_config,
                collision_world,
                &open_barrier_kinds.0,
                event.id,
            );
        }
    }
}

// Handle player being hit - apply camera shake or cuboid shake.
fn apply_player_hit(
    commands: &mut Commands,
    players: &PlayerMap,
    camera_query: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    client_settings: &ClientSettings,
    my_player_id: PlayerId,
    event: &SPlayerHit,
) {
    debug!("{} was hit", players.describe(&event.id));
    if let Some(player) = players.get(&event.id) {
        commands.entity(player.entity).insert(event.health);
    }
    let shake = client_settings.camera.shake;
    if event.id == my_player_id {
        let source = match event.kind {
            HitKind::Projectile => shake.projectile,
            HitKind::Beam => shake.laser,
        };
        if let Ok(camera_entity) = camera_query.single() {
            commands.entity(camera_entity).insert(CameraShake {
                timer: Timer::from_seconds(source.duration_secs, TimerMode::Once),
                intensity: source.intensity,
                dir_x: event.hit_dir_x,
                // Small vertical companion to the directional hit shake.
                dir_y: source.vertical_ratio,
                dir_z: event.hit_dir_z,
                offset_x: 0.0,
                offset_y: 0.0,
                offset_z: 0.0,
            });
        }
    } else if let Some(player) = players.get(&event.id) {
        commands.entity(player.entity).insert(CuboidShake {
            timer: Timer::from_seconds(0.3, TimerMode::Once),
            intensity: 0.3,
            dir_x: event.hit_dir_x,
            dir_z: event.hit_dir_z,
            offset_x: 0.0,
            offset_z: 0.0,
        });
    }
}

// Handle player death — the primary trigger for client-side death effects.
// For the local player: keep the entity (camera/look need it), hide it, set
// `is_dead`. For other players: despawn + drop `PlayerInfo`. The snapshot
// diff in `sync_players` is the idempotent fallback if this event was lost.
// The feed line arrives separately as an `SFeed`.
//
// Respawn is *not* handled here — `sync_players` clears `is_dead` and
// teleports the local entity when the player reappears in the next snapshot.
#[expect(
    clippy::too_many_arguments,
    reason = "death state dependencies stay explicit for tests"
)]
fn apply_player_death(
    commands: &mut Commands,
    ctx: &mut ExplosionSpawnCtx,
    blast_radii: &BlastRadii,
    players: &mut PlayerMap,
    local_player_info: &mut LocalPlayerInfo,
    banner: &mut HudBanner,
    my_player_id: PlayerId,
    event: &SPlayerDeath,
) {
    // Early-apply the victim's post-death score so the HUD bumps on the
    // death tick instead of waiting for the next snapshot. Same idea for
    // the killer's bonus (when there is one). Snapshot remains the system
    // of record; this just cuts the latency.
    if let Some(info) = players.get_mut(&event.id) {
        info.score = event.victim_score;
        // The server zeroes missiles in `clear_per_life_state`; mirror it
        // here so the ammo HUD resets on the death tick.
        info.missiles = 0;
    }
    if let (Some(killer_id), Some(killer_score)) = (event.killer, event.killer_score)
        && let Some(killer_info) = players.get_mut(&killer_id)
    {
        killer_info.score = killer_score;
    }

    if let Some(info) = players.get(&event.id) {
        commands.entity(info.entity).insert(Health(0.0));
    }

    // Positional, so it fires even if the victim isn't in `PlayerMap` yet.
    // For the local player the fireball's backfaces are culled, so the
    // first-person camera inside the sphere sees shards/ring/light rather
    // than an orange screen wash.
    spawn_player_explosion(commands, ctx, blast_radii, event.pos);

    if event.id == my_player_id {
        if let Some(info) = players.get(&event.id) {
            // Snap the kept (hidden) entity onto the server-authoritative death
            // position. Local prediction may have drifted from the server when
            // reconciliation hadn't converged, and the corpse stays visible in
            // the top-down death view, so park it on the true spot. Reset
            // `PreviousTickPosition` so render interpolation doesn't smear the
            // snap, and drop any in-flight `ServerReconciliation` so a stale
            // lerp can't pull the corpse back off the death position.
            commands
                .entity(info.entity)
                .insert((Visibility::Hidden, event.pos, PreviousTickPosition(event.pos)))
                .remove::<ServerReconciliation>();
        }
        local_player_info.is_dead = true;
        // Centered "You died!" banner. The red full-screen
        // `DeathOverlayMarker` tint and the feed line are independent
        // layers; the banner is the headline.
        banner.push(BannerMessage::Death);
    } else if let Some(info) = players.remove(&event.id) {
        commands.entity(info.entity).despawn();
    }
}

// Blast launch for the local player. The server already applied the same
// impulse authoritatively; prediction must integrate it too or the next
// reconciliation drags the launch back. Remote players need nothing — their
// motion arrives via snapshots.
// No camera shake here: the knockback the blast applies IS the feedback —
// shake on top reads as double impact. Shake is projectile-hits only.
fn apply_player_blast(commands: &mut Commands, players: &PlayerMap, my_player_id: PlayerId, event: &SPlayerBlast) {
    // Unicast to the victim, but stay defensive about routing.
    if event.id != my_player_id {
        return;
    }
    let Some(info) = players.get(&event.id) else {
        return;
    };
    commands.entity(info.entity).insert((
        event.health,
        CharacterVerticalVelocity(event.vertical_velocity),
        KnockbackVelocity(Vec3::new(event.velocity_x, 0.0, event.velocity_z)),
    ));
}

// Handle player status update (power-ups, stun).
fn apply_player_status(
    commands: &mut Commands,
    players: &mut PlayerMap,
    event: &SPlayerStatus,
    my_player_id: PlayerId,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
) {
    if let Some(player_info) = players.get_mut(&event.id) {
        // Play power-up sound effect only for the local player
        if event.id == my_player_id {
            // Don't play the power-up sound if this event is due to a stun change.
            if player_info.stunned == event.stunned {
                // Only play power-up sound effect if it wasn't a downgrade —
                // i.e., no kind transitioned from active to inactive.
                let lost_power_up = PowerUpKind::ALL
                    .iter()
                    .any(|kind| player_info.power_up(*kind) && !event.power_up(*kind));

                if !lost_power_up {
                    play_sound(commands, asset_server, asset_set.player_sound("collect_power_up"));
                }
            }
        }

        player_info.apply_status(event);
    }
}

// Player took fall damage. Updates HUD health on the impact frame (instead
// of waiting for the next snapshot), applies a vertical camera shake — same
// envelope as a projectile hit, re-aimed along the Y axis — and plays the
// landing thud. Unicast, so the event only ever targets the local player;
// no other-player branch.
#[expect(clippy::too_many_arguments, reason = "fall feedback dependencies stay explicit")]
fn apply_player_fall_damage(
    commands: &mut Commands,
    players: &PlayerMap,
    camera_query: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    client_settings: &ClientSettings,
    my_player_id: PlayerId,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    event: &SPlayerFallDamage,
) {
    if let Some(player) = players.get(&event.id) {
        commands.entity(player.entity).insert(event.health);
    }
    if event.id == my_player_id {
        let source = client_settings.camera.shake.fall;
        if let Ok(camera_entity) = camera_query.single() {
            commands.entity(camera_entity).insert(CameraShake {
                timer: Timer::from_seconds(source.duration_secs, TimerMode::Once),
                intensity: source.intensity,
                dir_x: 0.0,
                dir_y: source.vertical_ratio,
                dir_z: 0.0,
                offset_x: 0.0,
                offset_y: 0.0,
                offset_z: 0.0,
            });
        }
        play_sound(commands, asset_server, asset_set.player_sound("fall_damage"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        players::PlayerInfo,
        vfx::{ExplosionAssets, ExplosionVfxBudget},
    };

    fn player_info(entity: Entity, name: &str) -> PlayerInfo {
        PlayerInfo {
            entity,
            score: 0,
            name: name.to_owned(),
            power_ups: [false; PowerUpKind::COUNT],
            stunned: false,
            snap_speed: 0.0,
            held_keys: Vec::new(),
            missiles: 0,
        }
    }

    #[test]
    fn disabled_player_reconciliation_velocity_is_vertical_only() {
        let gameplay = GameplayConfig::load_default().expect("default gameplay config failed to load");
        let movement = PlayerMovementState::new(
            Position::default(),
            PlayerMoveIntent::Running { direction: 0.0 },
            -3.0,
            0.0,
        );

        assert_eq!(
            player_movement_velocity(movement, &gameplay, true, true),
            Vec3::new(0.0, -3.0, 0.0)
        );
    }

    #[test]
    fn local_player_death_sets_health_to_zero() {
        let my_id = PlayerId(7);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let entity = app.world_mut().spawn((Health(42.0), Visibility::Visible)).id();
        let world = app.world_mut();
        let mut players = PlayerMap::default();
        players.insert(my_id, player_info(entity, "Alice"));
        let mut local_player_info = LocalPlayerInfo::default();
        let mut banner = HudBanner::default();
        let gameplay_config = GameplayConfig::load_default().expect("default gameplay config failed to load");
        let mut mesh_assets = Assets::<Mesh>::default();
        let mut material_assets = Assets::<StandardMaterial>::default();
        let explosion_assets = ExplosionAssets::new(&mut mesh_assets, &mut material_assets);
        let mut explosion_budget = ExplosionVfxBudget::default();
        let blast_radii = BlastRadii::default();
        let mut commands_queue = bevy::ecs::world::CommandQueue::default();

        {
            let mut commands = bevy::ecs::system::Commands::new(&mut commands_queue, world);
            let mut ctx = ExplosionSpawnCtx {
                meshes: &mut mesh_assets,
                materials: &mut material_assets,
                budget: &mut explosion_budget,
                explosion_assets: &explosion_assets,
                gameplay_config: &gameplay_config,
                collision_world: None,
                map_layout: None,
            };
            apply_player_death(
                &mut commands,
                &mut ctx,
                &blast_radii,
                &mut players,
                &mut local_player_info,
                &mut banner,
                my_id,
                &SPlayerDeath {
                    id: my_id,
                    pos: Position::default(),
                    killer: None,
                    victim_score: 0,
                    killer_score: None,
                },
            );
        }
        commands_queue.apply(world);

        assert_eq!(world.entity(entity).get::<Health>(), Some(&Health(0.0)));
        assert_eq!(world.entity(entity).get::<Visibility>(), Some(&Visibility::Hidden));
        assert!(local_player_info.is_dead);
    }
}
