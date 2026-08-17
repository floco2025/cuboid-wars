use crate::constants::{BANNER_DEATH_SECS, BANNER_DEATH_TEXT};
use bevy::prelude::*;

use crate::{
    audio::play_sound,
    cameras::MainCameraMarker,
    characters::PreviousTickPosition,
    config::{AssetSet, ClientSettings},
    network::{RoundTripTime, ServerReconciliation},
    players::{CameraShake, CuboidShake, LocalPlayerInfo, PlayerMap},
    projectiles::{ProjectileAssets, spawn_projectiles},
    ui::{GameMessage, GameMessageFeed, PendingBanner},
    vfx::{ExplosionRadii, ExplosionSpawnCtx, spawn_player_explosion},
};
use common::{
    config::GameplayConfig,
    physics::{CharacterVerticalVelocity, CollisionWorld, KnockbackVelocity, OpenBarrierKinds},
    protocol::*,
};

// ============================================================================
// Player Message Handlers
// ============================================================================

pub(super) fn player_movement_velocity(
    movement: PlayerMovementState,
    gameplay_config: &GameplayConfig,
    has_speed_power_up: bool,
) -> Vec3 {
    let mut velocity = movement.move_intent.to_horizontal_velocity(
        gameplay_config.player.walk_speed,
        gameplay_config.player.run_speed,
        has_speed_power_up,
        gameplay_config.power_up_effects.speed_multiplier,
    );
    velocity.y = movement.vertical_velocity;
    velocity
}

// Handle player move-input update with server reconciliation.
pub fn handle_player_move_intent_message(
    commands: &mut Commands,
    players: &ResMut<PlayerMap>,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection), With<PlayerMarker>>,
    rtt: &ResMut<RoundTripTime>,
    gameplay_config: &GameplayConfig,
    msg: SPlayerMoveIntent,
) {
    trace!("{:?} move intent: {:?}", msg.id, msg);
    if let Some(player) = players.get(&msg.id) {
        let server_velocity =
            player_movement_velocity(msg.movement, gameplay_config, player.power_up(PowerUpKind::Speed));

        // Add server reconciliation if we have client position
        if let Ok((client_pos, _, _)) = player_data.get(player.entity) {
            commands.entity(player.entity).insert((
                msg.movement.move_intent, // Never the local player, so we can always overwrite intent
                ServerReconciliation::new(*client_pos, msg.movement.pos, server_velocity, rtt),
            ));
        } else {
            commands.entity(player.entity).insert(msg.movement.move_intent);
        }
    }
}

pub fn handle_player_jump_message(
    commands: &mut Commands,
    players: &ResMut<PlayerMap>,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection), With<PlayerMarker>>,
    rtt: &ResMut<RoundTripTime>,
    gameplay_config: &GameplayConfig,
    msg: SJump,
) {
    if let Some(player) = players.get(&msg.id)
        && let Ok((client_pos, _, _)) = player_data.get(player.entity)
    {
        let server_velocity =
            player_movement_velocity(msg.movement, gameplay_config, player.power_up(PowerUpKind::Speed));
        commands.entity(player.entity).insert((
            msg.movement.move_intent,
            CharacterVerticalVelocity(msg.movement.vertical_velocity),
            ServerReconciliation::new(*client_pos, msg.movement.pos, server_velocity, rtt),
        ));
    }
}

// Handle player face direction update.
pub fn handle_player_face_message(commands: &mut Commands, players: &ResMut<PlayerMap>, msg: SFace) {
    trace!("{:?} face direction: {}", msg.id, msg.dir);
    if let Some(player) = players.get(&msg.id) {
        commands.entity(player.entity).insert(FaceDirection(msg.dir));
    }
}

// Handle player shooting - spawn projectile(s) on client.
pub fn handle_player_shot_message(
    commands: &mut Commands,
    projectile_assets: &ProjectileAssets,
    players: &ResMut<PlayerMap>,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection), With<PlayerMarker>>,
    msg: SShot,
    collision_world: Option<&CollisionWorld>,
    gameplay_config: &GameplayConfig,
    open_barrier_kinds: &OpenBarrierKinds,
) {
    trace!("{:?} shot: {:?}", msg.id, msg);
    if let Some(player) = players.get(&msg.id) {
        commands.entity(player.entity).insert(FaceDirection(msg.face_dir));

        // Spawn projectile(s) based on player's multi-shot power-up status
        if let Ok((position, _, _)) = player_data.get(player.entity)
            && let Some(collision_world) = collision_world
        {
            spawn_projectiles(
                commands,
                projectile_assets,
                position,
                msg.face_dir,
                msg.face_pitch,
                player.power_up(PowerUpKind::MultiShot),
                gameplay_config.player.eye_height(),
                gameplay_config,
                collision_world,
                &open_barrier_kinds.0,
                msg.id,
            );
        }
    }
}

// Handle player being hit - apply camera shake or cuboid shake.
pub fn handle_player_hit_message(
    commands: &mut Commands,
    players: &ResMut<PlayerMap>,
    camera_query: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    client_settings: &ClientSettings,
    my_player_id: PlayerId,
    msg: SPlayerHit,
) {
    debug!("{} was hit", players.describe(&msg.id));
    if let Some(player) = players.get(&msg.id) {
        commands.entity(player.entity).insert(msg.health);
    }
    let shake = client_settings.camera.shake;
    if msg.id == my_player_id {
        let source = match msg.kind {
            HitKind::Projectile => shake.projectile,
            HitKind::Beam => shake.laser,
        };
        if let Ok(camera_entity) = camera_query.single() {
            commands.entity(camera_entity).insert(CameraShake {
                timer: Timer::from_seconds(source.duration_secs, TimerMode::Once),
                intensity: source.intensity,
                dir_x: msg.hit_dir_x,
                // Small vertical companion to the directional hit shake.
                dir_y: source.vertical_ratio,
                dir_z: msg.hit_dir_z,
                offset_x: 0.0,
                offset_y: 0.0,
                offset_z: 0.0,
            });
        }
    } else if let Some(player) = players.get(&msg.id) {
        commands.entity(player.entity).insert(CuboidShake {
            timer: Timer::from_seconds(0.3, TimerMode::Once),
            intensity: 0.3,
            dir_x: msg.hit_dir_x,
            dir_z: msg.hit_dir_z,
            offset_x: 0.0,
            offset_z: 0.0,
        });
    }
}

// Handle player death — the primary trigger for client-side death effects.
// For the local player: keep the entity (camera/look need it), hide it, set
// `is_dead`. For other players: despawn + drop `PlayerInfo`. The snapshot
// diff in `sync_players` is the idempotent fallback if this event was lost.
//
// Respawn is *not* handled here — `sync_players` clears `is_dead` and
// teleports the local entity when the player reappears in the next snapshot.
#[expect(clippy::too_many_arguments, reason = "message handler threading dispatcher state")]
pub fn handle_player_death_message(
    commands: &mut Commands,
    ctx: &mut ExplosionSpawnCtx,
    explosion_radii: &ExplosionRadii,
    players: &mut PlayerMap,
    local_player_info: &mut LocalPlayerInfo,
    feed: &mut GameMessageFeed,
    _client_settings: &ClientSettings,
    pending_banner: &mut PendingBanner,
    my_player_id: PlayerId,
    msg: SPlayerDeath,
) {
    let victim_name = players.get(&msg.id).map(|info| info.name.clone());
    let killer_name = msg
        .killer
        .and_then(|killer_id| players.get(&killer_id))
        .map(|info| info.name.clone());

    if let Some(victim_name) = victim_name {
        match killer_name {
            Some(killer_name) => feed.push(GameMessage::Kill {
                killer_name,
                victim_name,
            }),
            None => feed.push(GameMessage::SoloDeath {
                player_name: victim_name,
            }),
        }
    }

    // Early-apply the victim's post-death score so the HUD bumps on the
    // death tick instead of waiting for the next snapshot. Same idea for
    // the killer's bonus (when there is one). Snapshot remains the system
    // of record; this just cuts the latency.
    if let Some(info) = players.get_mut(&msg.id) {
        info.score = msg.victim_score;
        // The server zeroes missiles in `clear_per_life_state`; mirror it
        // here so the ammo HUD resets on the death tick.
        info.missiles = 0;
    }
    if let (Some(killer_id), Some(killer_score)) = (msg.killer, msg.killer_score)
        && let Some(killer_info) = players.get_mut(&killer_id)
    {
        killer_info.score = killer_score;
    }

    if let Some(info) = players.get(&msg.id) {
        commands.entity(info.entity).insert(Health(0.0));
    }

    // Positional, so it fires even if the victim isn't in `PlayerMap` yet.
    // For the local player the fireball's backfaces are culled, so the
    // first-person camera inside the sphere sees shards/ring/light rather
    // than an orange screen wash.
    spawn_player_explosion(commands, ctx, explosion_radii, msg.pos);

    if msg.id == my_player_id {
        if let Some(info) = players.get(&msg.id) {
            // Snap the kept (hidden) entity onto the server-authoritative death
            // position. Local prediction may have drifted from the server when
            // reconciliation hadn't converged, and the corpse stays visible in
            // the top-down death view, so park it on the true spot. Reset
            // `PreviousTickPosition` so render interpolation doesn't smear the
            // snap, and drop any in-flight `ServerReconciliation` so a stale
            // lerp can't pull the corpse back off the death position.
            commands
                .entity(info.entity)
                .insert((Visibility::Hidden, msg.pos, PreviousTickPosition(msg.pos)))
                .remove::<ServerReconciliation>();
        }
        local_player_info.is_dead = true;
        // Centered "You have died!" banner. The red full-screen
        // `DeathOverlayMarker` tint and the message-feed `SoloDeath`
        // entry are independent layers; the banner is the headline.
        pending_banner.set(BANNER_DEATH_TEXT.to_owned(), BANNER_DEATH_SECS);
    } else if let Some(info) = players.remove(&msg.id) {
        commands.entity(info.entity).despawn();
    }
}

// Blast launch for the local player. The server already applied the same
// impulse authoritatively; prediction must integrate it too or the next
// reconciliation drags the launch back. Remote players need nothing — their
// motion arrives via snapshots.
// No camera shake here: the knockback the blast applies IS the feedback —
// shake on top reads as double impact. Shake is projectile-hits only.
pub fn handle_player_blast_message(
    commands: &mut Commands,
    players: &PlayerMap,
    my_player_id: PlayerId,
    msg: SPlayerBlast,
) {
    // Unicast to the victim, but stay defensive about routing.
    if msg.id != my_player_id {
        return;
    }
    let Some(info) = players.get(&msg.id) else {
        return;
    };
    commands.entity(info.entity).insert((
        msg.health,
        CharacterVerticalVelocity(msg.vertical_velocity),
        KnockbackVelocity(Vec3::new(msg.velocity_x, 0.0, msg.velocity_z)),
    ));
}

// Handle player status update (power-ups, stun).
pub fn handle_player_status_message(
    commands: &mut Commands,
    players: &mut ResMut<PlayerMap>,
    feed: &mut GameMessageFeed,
    msg: SPlayerStatus,
    my_player_id: PlayerId,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
) {
    if let Some(player_info) = players.get_mut(&msg.id) {
        // Emit a feed entry for each key the player just gained. New keys
        // are those in the message but not in the locally-mirrored set.
        // The kind id itself is internal — the renderer just uses it to
        // pick a color for the word "key"; no internal name shown.
        for new_kind in &msg.held_keys {
            if !player_info.held_keys.contains(new_kind) {
                feed.push(GameMessage::KeyFound {
                    player_name: player_info.name.clone(),
                    kind: *new_kind,
                });
            }
        }
        // Play power-up sound effect only for the local player
        if msg.id == my_player_id {
            // Don't play power-up sound effect if this message is due to a stun change
            if player_info.stunned == msg.stunned {
                // Only play power-up sound effect if it wasn't a downgrade —
                // i.e., no kind transitioned from active to inactive.
                let lost_power_up = PowerUpKind::ALL
                    .iter()
                    .any(|kind| player_info.power_up(*kind) && !msg.power_up(*kind));

                if !lost_power_up {
                    play_sound(commands, asset_server, asset_set.player_sound("collect_power_up"));
                }
            }
        }

        player_info.apply_status(&msg);
    }
}

// Player took fall damage. Updates HUD health on the impact frame (instead
// of waiting for the next snapshot), applies a vertical camera shake — same
// envelope as a projectile hit, re-aimed along the Y axis — and plays the
// landing thud. Unicast, so the message only ever targets the local player;
// no other-player branch.
#[expect(clippy::too_many_arguments, reason = "message handler threading dispatcher state")]
pub fn handle_fall_damage_message(
    commands: &mut Commands,
    players: &ResMut<PlayerMap>,
    camera_query: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    client_settings: &ClientSettings,
    my_player_id: PlayerId,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    msg: SPlayerFallDamage,
) {
    if let Some(player) = players.get(&msg.id) {
        commands.entity(player.entity).insert(msg.health);
    }
    if msg.id == my_player_id {
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
    fn local_player_death_sets_health_to_zero() {
        let my_id = PlayerId(7);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let entity = app.world_mut().spawn((Health(42.0), Visibility::Visible)).id();
        let world = app.world_mut();
        let mut players = PlayerMap::default();
        players.insert(my_id, player_info(entity, "Alice"));
        let mut local_player_info = LocalPlayerInfo::default();
        let mut feed = GameMessageFeed::default();
        let mut pending_banner = PendingBanner::default();
        let client_settings = ClientSettings::load_default().expect("load default client settings");
        let gameplay_config = GameplayConfig::load_default().expect("load default gameplay config");
        let mut mesh_assets = Assets::<Mesh>::default();
        let mut material_assets = Assets::<StandardMaterial>::default();
        let explosion_assets = ExplosionAssets::new(&mut mesh_assets, &mut material_assets);
        let mut explosion_budget = ExplosionVfxBudget::default();
        let explosion_radii = ExplosionRadii::default();
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
            handle_player_death_message(
                &mut commands,
                &mut ctx,
                &explosion_radii,
                &mut players,
                &mut local_player_info,
                &mut feed,
                &client_settings,
                &mut pending_banner,
                my_id,
                SPlayerDeath {
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
