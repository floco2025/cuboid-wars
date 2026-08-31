use bevy::prelude::*;

use super::super::context::ServerMessageContext;
use crate::constants::RECON_TELEPORT_SUPPRESS_SECS;
use crate::{
    audio::{play_explosion_sound, play_sound},
    characters::PreviousTickPosition,
    network::ServerReconciliation,
    players::{CameraShake, CuboidShake, LocalPlayerInfo, PlayerMap},
    projectiles::spawn_projectiles,
    ui::{BannerMessage, HudBanner},
    vfx::spawn_player_explosion,
};
use common::{
    config::GameplayConfig,
    physics::{CharacterVerticalVelocity, KnockbackVelocity, player_control_velocity},
    protocol::*,
};

// Handle player move update (intent + facing) with server reconciliation.
pub(in crate::network) fn handle_player_move_message(
    message: SPlayerMove,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    trace!("{:?} move: {:?}", message.id, message);
    if let Some(player) = context.players.get(&message.id) {
        let server_velocity = player_movement_velocity(
            message.movement,
            &context.gameplay_config,
            player.power_up(PowerUpKind::Speed),
            player.stunned,
        );

        // Never the local player, so we can always overwrite intent + facing.
        let input = (message.movement.move_intent, FaceYaw(message.movement.face_yaw));
        // Reconciliation stands down after a teleport cue: this move's data
        // may predate the teleport (`RECON_TELEPORT_SUPPRESS_SECS`).
        let suppress_recon = context.time.elapsed_secs() - player.last_teleport_time < RECON_TELEPORT_SUPPRESS_SECS;
        if let Ok((client_pos, _, _)) = context.player_data.get(player.entity)
            && !suppress_recon
        {
            commands.entity(player.entity).insert((
                input,
                ServerReconciliation::new(*client_pos, message.movement.pos, server_velocity, &context.rtt),
            ));
        } else {
            commands.entity(player.entity).insert(input);
        }
    }
}

pub(in crate::network) fn handle_player_jump_message(
    message: SPlayerJump,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    if let Some(player) = context.players.get(&message.id)
        && let Ok((client_pos, _, _)) = context.player_data.get(player.entity)
    {
        let server_velocity = player_movement_velocity(
            message.movement,
            &context.gameplay_config,
            player.power_up(PowerUpKind::Speed),
            player.stunned,
        );
        commands.entity(player.entity).insert((
            message.movement.move_intent,
            FaceYaw(message.movement.face_yaw),
            CharacterVerticalVelocity(message.movement.vertical_velocity),
        ));
        // Same teleport stand-down as the move stream.
        if context.time.elapsed_secs() - player.last_teleport_time >= RECON_TELEPORT_SUPPRESS_SECS {
            commands.entity(player.entity).insert(ServerReconciliation::new(
                *client_pos,
                message.movement.pos,
                server_velocity,
                &context.rtt,
            ));
        }
    }
}

// Handle player shooting - spawn projectile(s) on client.
pub(in crate::network) fn handle_player_shot_message(
    message: SPlayerShot,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    trace!("{:?} shot: {:?}", message.id, message);
    if let Some(player) = context.players.get(&message.id) {
        commands.entity(player.entity).insert(FaceYaw(message.face_yaw));

        // Spawn projectile(s) based on player's multi-shot power-up status
        if let Ok((position, _, _)) = context.player_data.get(player.entity)
            && let Some(collision_world) = context.collision_world.as_deref()
        {
            spawn_projectiles(
                commands,
                &context.projectile_assets,
                position,
                message.face_yaw,
                message.face_pitch,
                player.power_up(PowerUpKind::MultiShot),
                context.gameplay_config.player.eye_height(),
                &context.gameplay_config,
                collision_world,
                &context.open_barrier_kinds.0,
                message.id,
            );
        }
    }
}

pub(in crate::network) fn handle_player_death_message(
    message: SPlayerDeath,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
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
    // Positional, so it fires even if the victim isn't in `PlayerMap` yet.
    // For the local player the fireball's backfaces are culled, so the
    // first-person camera inside the sphere sees shards/ring/light rather
    // than an orange screen wash.
    spawn_player_explosion(commands, &mut context.explosion_ctx(), message.pos);
    apply_player_death(
        commands,
        &mut context.players,
        &mut context.local_player_info,
        &mut context.banner,
        my_player_id,
        message,
    );
}

// Handle player being hit - apply camera shake or cuboid shake.
pub(in crate::network) fn handle_player_hit_message(
    message: SPlayerHit,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
) {
    debug!("{} was hit", context.players.describe(&message.id));
    if let Some(player) = context.players.get(&message.id) {
        commands.entity(player.entity).insert(message.health);
    }
    let shake = context.client_settings.camera.shake;
    if message.id == my_player_id {
        let source = match message.kind {
            HitKind::Projectile => shake.projectile,
            HitKind::Beam => shake.laser,
        };
        if let Ok(camera_entity) = context.cameras.single() {
            commands.entity(camera_entity).insert(CameraShake {
                timer: Timer::from_seconds(source.duration_secs, TimerMode::Once),
                intensity: source.intensity * shake.scale,
                dir_x: message.hit_dir_x,
                // Small vertical companion to the directional hit shake.
                dir_y: source.vertical_ratio,
                dir_z: message.hit_dir_z,
                offset_x: 0.0,
                offset_y: 0.0,
                offset_z: 0.0,
            });
        }
    } else if let Some(player) = context.players.get(&message.id) {
        commands.entity(player.entity).insert(CuboidShake {
            timer: Timer::from_seconds(0.3, TimerMode::Once),
            intensity: 0.3,
            dir_x: message.hit_dir_x,
            dir_z: message.hit_dir_z,
            offset_x: 0.0,
            offset_z: 0.0,
        });
    }
}

// Player took fall damage. Updates HUD health on the impact frame (instead
// of waiting for the next snapshot), applies a vertical camera shake — same
// envelope as a projectile hit, re-aimed along the Y axis — and plays the
// landing thud. Unicast, so the event only ever targets the local player;
// no other-player branch.
pub(in crate::network) fn handle_player_fall_damage_message(
    message: SPlayerFallDamage,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
) {
    if let Some(player) = context.players.get(&message.id) {
        commands.entity(player.entity).insert(message.health);
    }
    if message.id == my_player_id {
        let shake = context.client_settings.camera.shake;
        let source = shake.fall;
        if let Ok(camera_entity) = context.cameras.single() {
            commands.entity(camera_entity).insert(CameraShake {
                timer: Timer::from_seconds(source.duration_secs, TimerMode::Once),
                intensity: source.intensity * shake.scale,
                dir_x: 0.0,
                dir_y: source.vertical_ratio,
                dir_z: 0.0,
                offset_x: 0.0,
                offset_y: 0.0,
                offset_z: 0.0,
            });
        }
        play_sound(
            commands,
            &context.asset_server,
            context.asset_set.player_sound("fall_damage"),
        );
    }
}

// Blast launch for the local player. The server already applied the same
// impulse authoritatively; prediction must integrate it too or the next
// reconciliation drags the launch back. Remote players need nothing — their
// motion arrives via snapshots.
// No camera shake here: the knockback the blast applies IS the feedback —
// shake on top reads as double impact. Shake is projectile-hits only.
pub(in crate::network) fn handle_player_blast_message(
    message: SPlayerBlast,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
) {
    // Unicast to the victim, but stay defensive about routing.
    if message.id != my_player_id {
        return;
    }
    let Some(info) = context.players.get(&message.id) else {
        return;
    };
    commands.entity(info.entity).insert((
        message.health,
        CharacterVerticalVelocity(message.vertical_velocity),
        KnockbackVelocity(Vec3::new(message.velocity_x, 0.0, message.velocity_z)),
    ));
}

// Handle player status update (power-ups, stun).
pub(in crate::network) fn handle_player_status_message(
    message: SPlayerStatus,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
) {
    if let Some(player_info) = context.players.get_mut(&message.id) {
        // Play power-up sound effect only for the local player
        if message.id == my_player_id {
            // Don't play the power-up sound if this event is due to a stun change.
            if player_info.stunned == message.stunned {
                // Only play power-up sound effect if it wasn't a downgrade —
                // i.e., no kind transitioned from active to inactive.
                let lost_power_up = PowerUpKind::ALL
                    .iter()
                    .any(|kind| player_info.power_up(*kind) && !message.power_up(*kind));

                if !lost_power_up {
                    play_sound(
                        commands,
                        &context.asset_server,
                        context.asset_set.player_sound("collect_power_up"),
                    );
                }
            }
        }

        player_info.apply_status(&message);
    }
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

// Handle player death — the primary trigger for client-side death effects.
// For the local player: keep the entity (camera/look need it), hide it, set
// `is_dead`. For other players: despawn + drop `PlayerInfo`. The snapshot
// diff in `sync_players` is the idempotent fallback if this event was lost.
// The feed line arrives separately as an `SFeed`.
//
// Respawn is *not* handled here — `sync_players` clears `is_dead` and
// teleports the local entity when the player reappears in the next snapshot.
fn apply_player_death(
    commands: &mut Commands,
    players: &mut PlayerMap,
    local_player_info: &mut LocalPlayerInfo,
    banner: &mut HudBanner,
    my_player_id: PlayerId,
    event: SPlayerDeath,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::players::PlayerInfo;

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
            last_teleport_time: f32::NEG_INFINITY,
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
        let mut commands_queue = bevy::ecs::world::CommandQueue::default();

        {
            let mut commands = bevy::ecs::system::Commands::new(&mut commands_queue, world);
            apply_player_death(
                &mut commands,
                &mut players,
                &mut local_player_info,
                &mut banner,
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
