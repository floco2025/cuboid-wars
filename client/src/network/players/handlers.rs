use bevy::prelude::*;

use super::super::context::ServerMessageContext;
use crate::constants::HOP_DISPUTE_SLACK_SECS;
use crate::{
    audio::{play_explosion_sound, play_sound, play_spatial_sound},
    characters::PreviousTickPosition,
    network::{ServerReconciliation, extrapolated_correction, recorded_correction},
    players::{CameraShake, CuboidShake, LocalPlayerInfo, PlayerMap},
    projectiles::spawn_projectiles,
    ui::{BannerMessage, HudBanner},
    vfx::spawn_player_explosion,
};
use common::{
    config::MapMovementConfig,
    constants::TICK_HZ,
    physics::{CharacterVerticalVelocity, KnockbackVelocity, PortalMomentum, player_control_velocity},
    protocol::*,
};

// This tick's movement state of every player, with server reconciliation.
// The local player's intent, facing, and vertical velocity are its own; only
// the reconciliation applies to it.
pub(in crate::network) fn handle_player_moves_message(
    message: SPlayerMoves,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
) {
    trace!("moves: {:?}", message);
    if !sequence_is_newer(message.seq, context.last_player_moves_seq.0) {
        debug!(
            "ignoring outdated player moves (seq {}, last {})",
            message.seq, context.last_player_moves_seq.0
        );
        return;
    }
    context.last_player_moves_seq.0 = message.seq;
    // A crossing the server has yet to make shows up within a round trip;
    // one still missing after that was mispredicted, and the server's side
    // stands. Until the round trip has been measured no dispute can be
    // judged, so none is.
    let dispute_limit = (!context.rtt.measurements.is_empty())
        .then(|| ((context.rtt.rtt.as_secs_f32() + HOP_DISPUTE_SLACK_SECS) * TICK_HZ as f32) as u32);
    for PlayerMove {
        id,
        movement,
        move_seq,
        hops,
    } in message.moves
    {
        let Some(player) = context.players.get_mut(&id) else {
            continue;
        };
        // A state pairs only with a simulation on the same side of the same
        // portal crossings; one from across a crossing we have predicted, or
        // not yet predicted, would steer and reconcile the body back through.
        if hops != player.hops {
            player.disputed_echoes += 1;
            if dispute_limit.is_none_or(|limit| player.disputed_echoes <= limit) {
                continue;
            }
            // The server's side stands: put the player there outright. The
            // gap between two portals can sit under the snap threshold, and a
            // vertical gap is never eased, so ordinary reconciliation could
            // leave the player on the wrong side with the right count.
            warn!(
                "{} crossing dispute settled for the server: {} hops there, {} here; teleporting",
                player.name, hops, player.hops
            );
            player.hops = hops;
            player.disputed_echoes = 0;
            let mut entity = commands.entity(player.entity);
            entity
                .insert((
                    movement.pos,
                    PreviousTickPosition(movement.pos),
                    CharacterVerticalVelocity(movement.vertical_velocity),
                ))
                .remove::<(ServerReconciliation, PortalMomentum)>();
            if id == my_player_id {
                context.local_player_info.committed_positions.clear();
            } else {
                entity.insert((movement.move_intent, FaceYaw(movement.face_yaw)));
            }
            continue;
        }
        player.disputed_echoes = 0;
        let mut entity = commands.entity(player.entity);
        if id != my_player_id {
            entity.insert((
                movement.move_intent,
                FaceYaw(movement.face_yaw),
                CharacterVerticalVelocity(movement.vertical_velocity),
            ));
        }
        if let Ok((client_pos, _, _)) = context.player_data.get(player.entity) {
            let server_velocity = player_movement_velocity(
                movement,
                &context.map_settings.movement,
                player.power_up(PowerUpKind::Speed),
                player.stunned,
            );
            let correction_delta = if id == my_player_id {
                // Own state names the `CMove` it reflects: measure against
                // where our simulation stood after that `CMove`. One the ring
                // does not hold (before the first commit, after a snap, or
                // after a one-way stall longer than the ring) is measured
                // against where we stand now, the plain gap to the server.
                let recorded_pos = context
                    .local_player_info
                    .committed_positions
                    .get(move_seq, hops)
                    .unwrap_or(*client_pos);
                recorded_correction(recorded_pos, movement.pos)
            } else {
                extrapolated_correction(*client_pos, movement.pos, server_velocity, &context.rtt)
            };
            entity.insert(ServerReconciliation::new(
                correction_delta,
                movement.pos,
                server_velocity,
                &context.rtt,
            ));
        }
    }
}

pub(in crate::network) fn handle_projectile_shot_message(
    message: SProjectileShot,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    trace!("{:?} shot: {:?}", message.id, message);
    if let Some(player) = context.players.get(&message.id) {
        commands.entity(player.entity).insert(FaceYaw(message.face_yaw));

        // `pattern` is already server-resolved against the shooter's power-up.
        if let Ok((position, _, _)) = context.player_data.get(player.entity)
            && spawn_projectiles(
                commands,
                &context.projectile_assets,
                position,
                message.face_yaw,
                message.face_pitch,
                message.pattern.as_deref(),
                context.gameplay_config.player.eye_height(),
                &context.gameplay_config,
                context.map_settings.movement.projectile_speed,
                &context.collision_world,
                &context.plates.open_barrier_kinds,
                message.id,
            ) > 0
        {
            // The excluded shooter already heard flat feedback; observers hear the muzzle instead.
            play_spatial_sound(
                commands,
                &context.asset_server,
                context.asset_set.player_sound("fire"),
                &context.client_settings.audio,
                Vec3::new(
                    position.x,
                    position.y + context.gameplay_config.player.eye_height(),
                    position.z,
                ),
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
    map_movement: &MapMovementConfig,
    has_speed_power_up: bool,
    movement_disabled: bool,
) -> Vec3 {
    let mut velocity = player_control_velocity(
        movement.move_intent,
        map_movement,
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
                .remove::<(ServerReconciliation, PortalMomentum)>();
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
    use common::config::{KnockbackConfig, PlayerMovementConfig};
    use std::collections::HashMap;

    fn movement_config() -> MapMovementConfig {
        MapMovementConfig {
            player: PlayerMovementConfig {
                walk_speed: 6.0,
                run_speed: 9.0,
                speed_power_up: 1.6,
                jump_speed: 12.0,
            },
            actors: HashMap::new(),
            missile_speed: 16.0,
            projectile_speed: 90.0,
            gravity: 25.0,
            low_gravity: 5.0,
            ladder_climb_ratio: 0.4,
            knockback: KnockbackConfig {
                max_speed: 15.0,
                up_speed: 7.0,
                deceleration: 35.0,
            },
        }
    }

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
            hops: 0,
            disputed_echoes: 0,
        }
    }

    #[test]
    fn disabled_player_reconciliation_velocity_is_vertical_only() {
        let map_movement = movement_config();
        let movement = PlayerMovementState::new(
            Position::default(),
            PlayerMoveIntent::Running { direction: 0.0 },
            -3.0,
            0.0,
        );

        assert_eq!(
            player_movement_velocity(movement, &map_movement, true, true),
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
