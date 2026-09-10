use bevy::prelude::*;

use super::super::context::ServerMessageContext;
use crate::{
    audio::{play_explosion_sound, play_sound, play_spatial_sound},
    characters::PreviousTickPosition,
    players::{CameraShake, CuboidShake, LocalPlayerInfo, PlayerMap},
    ui::{BannerMessage, HudBanner},
    vfx::spawn_player_explosion,
};
use common::{
    physics::{AirborneMomentum, CharacterVerticalVelocity, KnockbackVelocity},
    protocol::*,
};

pub(in crate::network) fn handle_player_death_message(
    message: SPlayerDeath,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
) {
    if !context.players.accepts_death(message.id, message.generation) {
        return;
    }
    // Keep audio outside the state handler so its unit test does not need an asset server.
    if message.effect == PlayerDeathEffect::Explosion {
        play_explosion_sound(
            commands,
            &context.assets.asset_server,
            context.assets.asset_set.player_sound("explodes"),
            &context.client_settings.audio,
            Vec3::from(message.pos),
            Some(context.assets.blast_radii.player),
        );
        // Positional, so it fires even if the victim isn't in `PlayerMap` yet.
        // For the local player the fireball's backfaces are culled, so the
        // first-person camera inside the sphere sees shards/ring/light rather
        // than an orange screen wash.
        spawn_player_explosion(commands, &mut context.explosion_ctx(), message.pos);
    } else if message.effect == PlayerDeathEffect::VoidFall && message.id == my_player_id {
        play_sound(
            commands,
            &context.assets.asset_server,
            context.assets.asset_set.player_sound("void_fall"),
        );
    } else if message.effect == PlayerDeathEffect::VoidFall {
        play_spatial_sound(
            commands,
            &context.assets.asset_server,
            context.assets.asset_set.player_sound("void_fall"),
            &context.client_settings.audio,
            Vec3::from(message.pos),
        );
    }
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
    if !context.players.accepts_body_cue(message.id, message.generation) {
        return;
    }
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
                intensity: source.intensity * context.client_settings.preferences.shake_scale,
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
    if !context.players.accepts_body_cue(message.id, message.generation) {
        return;
    }
    if let Some(player) = context.players.get(&message.id) {
        commands.entity(player.entity).insert(message.health);
    }
    if message.id == my_player_id {
        let shake = context.client_settings.camera.shake;
        let source = shake.fall;
        if let Ok(camera_entity) = context.cameras.single() {
            commands.entity(camera_entity).insert(CameraShake {
                timer: Timer::from_seconds(source.duration_secs, TimerMode::Once),
                intensity: source.intensity * context.client_settings.preferences.shake_scale,
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
            &context.assets.asset_server,
            context.assets.asset_set.player_sound("fall_damage"),
        );
    }
}

// Blast launch for the local player. The server adopts this client's next
// accepted report whole, so the impulse lives on only if prediction applies
// it here. Remote players need nothing — their motion arrives with the
// movement stream.
// No camera shake here: the knockback the blast applies IS the feedback —
// shake on top reads as double impact. Shake is projectile-hits only.
pub(in crate::network) fn handle_player_knockback_message(
    message: SPlayerKnockback,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
) {
    if !context.players.accepts_body_cue(message.id, message.generation) {
        return;
    }
    // Unicast to the victim, but stay defensive about routing.
    if message.id != my_player_id {
        return;
    }
    let Some(info) = context.players.get(&message.id) else {
        return;
    };
    let max_speed = context.map_settings.movement.knockback.max_speed * 1.5;
    commands.entity(info.entity).queue(move |entity: EntityWorldMut| {
        apply_player_impulse(entity, message, max_speed);
    });
}

fn apply_player_impulse(mut entity: EntityWorldMut, message: SPlayerKnockback, max_speed: f32) {
    let impulse = Vec3::from_array(message.impulse);
    if let Some(mut vertical) = entity.get_mut::<CharacterVerticalVelocity>() {
        vertical.0 += impulse.y;
    }
    if let Some(mut knockback) = entity.get_mut::<KnockbackVelocity>() {
        knockback.0 = (knockback.0 + Vec3::new(impulse.x, 0.0, impulse.z)).clamp_length_max(max_speed);
    }
    entity.insert(message.health);
}

pub(in crate::network) fn handle_equipment_erased_message(commands: &mut Commands, context: &ServerMessageContext) {
    play_sound(
        commands,
        &context.assets.asset_server,
        context.assets.asset_set.player_sound("eraser"),
    );
}

pub(in crate::network) fn handle_player_status_message(
    message: SPlayerStatus,
    commands: &mut Commands,
    my_player_id: PlayerId,
    context: &mut ServerMessageContext,
) {
    if !context.players.accepts_body_cue(message.id, message.generation) {
        return;
    }
    if let Some(player_info) = context.players.get_mut(&message.id) {
        if message.id == my_player_id
            && let Some(item) = message.collected
        {
            context.pending_weapon_selection.collect(item);
            play_sound(
                commands,
                &context.assets.asset_server,
                context.assets.asset_set.player_sound("collect_power_up"),
            );
        }

        player_info.apply_status(&message);
    }
}

// Handle player death — the primary trigger for client-side death effects.
// For the local player: keep the entity (camera/look need it), hide it, set
// `is_dead`. For other players: despawn + drop `PlayerInfo`. The snapshot
// diff in `sync_players` is the idempotent fallback if this event was lost.
// The feed line arrives separately as an `SFeed`.
//
// Spawns and relocations establish each body generation in `players/sync.rs`.
fn apply_player_death(
    commands: &mut Commands,
    players: &mut PlayerMap,
    local_player_info: &mut LocalPlayerInfo,
    banner: &mut HudBanner,
    my_player_id: PlayerId,
    event: SPlayerDeath,
) {
    if !players.retire_body(event.id, event.generation) {
        return;
    }
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
            // Keep the death view anchored to the server's death position.
            commands.entity(info.entity).insert((
                Visibility::Hidden,
                event.pos,
                PreviousTickPosition(event.pos),
                AirborneMomentum::default(),
            ));
        }
        local_player_info.is_dead = true;
        local_player_info.reports.clear_crossings();
        banner.push(if event.effect == PlayerDeathEffect::GroupRespawn {
            BannerMessage::GroupRespawn
        } else {
            BannerMessage::Death
        });
    } else if let Some(info) = players.remove(&event.id) {
        commands.entity(info.entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::players::PlayerInfo;
    use bevy::ecs::world::CommandQueue;

    #[test]
    fn queued_impulses_add_to_current_motion_and_clamp_the_planar_sum() {
        let mut world = World::new();
        let entity = world
            .spawn((
                CharacterVerticalVelocity(-6.0),
                KnockbackVelocity(Vec3::X * 2.0),
                Health(100.0),
            ))
            .id();
        let mut queue = CommandQueue::default();
        {
            let mut commands = Commands::new(&mut queue, &world);
            for health in [90.0, 80.0] {
                commands.entity(entity).queue(move |entity: EntityWorldMut| {
                    apply_player_impulse(
                        entity,
                        SPlayerKnockback {
                            id: PlayerId(1),
                            generation: PlayerGeneration(0),
                            health: Health(health),
                            impulse: [3.0, 4.0, 0.0],
                        },
                        10.0,
                    );
                });
            }
        }
        queue.apply(&mut world);
        assert_eq!(
            world
                .get::<CharacterVerticalVelocity>(entity)
                .expect("vertical velocity missing")
                .0,
            2.0
        );
        assert_eq!(
            world.get::<KnockbackVelocity>(entity).expect("knockback missing").0,
            Vec3::X * 8.0
        );
        assert_eq!(world.get::<Health>(entity).expect("health missing").0, 80.0);
        apply_player_impulse(
            world.entity_mut(entity),
            SPlayerKnockback {
                id: PlayerId(1),
                generation: PlayerGeneration(0),
                health: Health(70.0),
                impulse: [100.0, 30.0, 0.0],
            },
            10.0,
        );
        assert_eq!(
            world
                .get::<CharacterVerticalVelocity>(entity)
                .expect("vertical velocity missing")
                .0,
            32.0
        );
        assert_eq!(
            world.get::<KnockbackVelocity>(entity).expect("knockback missing").0,
            Vec3::X * 10.0
        );
    }

    fn player_info(entity: Entity, name: &str) -> PlayerInfo {
        PlayerInfo {
            generation: PlayerGeneration(0),
            entity,
            score: 0,
            name: name.to_owned(),
            power_ups: [false; PowerUpKind::COUNT],
            stunned: false,
            held_keys: Vec::new(),
            missiles: 0,
            last_movement_tick: 0,
            spawn_tick: 0,
        }
    }

    #[test]
    fn death_and_group_respawn_hide_the_player_with_the_matching_banner() {
        for snapshot_first in [false, true] {
            for effect in [
                PlayerDeathEffect::Explosion,
                PlayerDeathEffect::VoidFall,
                PlayerDeathEffect::GroupRespawn,
            ] {
                let my_id = PlayerId(7);
                let mut app = App::new();
                app.add_plugins(MinimalPlugins);
                let entity = app.world_mut().spawn((Health(42.0), Visibility::Visible)).id();
                let world = app.world_mut();
                let mut players = PlayerMap::default();
                players.insert(my_id, player_info(entity, "Alice"));
                let mut local_player_info = LocalPlayerInfo::default();
                if snapshot_first {
                    players.retire_body(my_id, PlayerGeneration(0));
                    local_player_info.is_dead = true;
                    world.entity_mut(entity).insert(Visibility::Hidden);
                }
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
                            generation: PlayerGeneration(0),
                            pos: Position::default(),
                            killer: None,
                            victim_score: 0,
                            killer_score: None,
                            effect,
                        },
                    );
                }
                commands_queue.apply(world);

                assert_eq!(world.entity(entity).get::<Health>(), Some(&Health(0.0)));
                assert_eq!(world.entity(entity).get::<Visibility>(), Some(&Visibility::Hidden));
                assert!(local_player_info.is_dead);
                assert_eq!(
                    banner.pending_texts(),
                    [if effect == PlayerDeathEffect::GroupRespawn {
                        "Group respawning"
                    } else {
                        "You died!"
                    }]
                );
            }
        }
    }
}
