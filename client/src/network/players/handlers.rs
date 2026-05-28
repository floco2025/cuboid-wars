use bevy::prelude::*;

use crate::{
    cameras::MainCameraMarker,
    config::{AssetSet, ClientSettings},
    network::{RoundTripTime, ServerReconciliation},
    players::{CameraShake, CuboidShake, LocalPlayerInfo, PlayerMap},
    projectiles::{ProjectileAssets, spawn_projectiles},
    ui::{ActiveQuests, GameMessage, GameMessageFeed, HudBannerMarker, spawn_hud_banner},
};
use common::{
    config::GameplayConfig,
    physics::{CharacterVerticalVelocity, CollisionWorld, OpenBarrierKinds},
    protocol::*,
};

// ============================================================================
// Player Message Handlers
// ============================================================================

pub(super) fn player_movement_velocity(
    movement: PlayerMovementState,
    walk_speed: f32,
    run_speed: f32,
    has_speed_power_up: bool,
) -> Vec3 {
    let mut velocity = movement
        .move_intent
        .to_horizontal_velocity(walk_speed, run_speed, has_speed_power_up);
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
        let server_velocity = player_movement_velocity(
            msg.movement,
            gameplay_config.player.walk_speed,
            gameplay_config.player.run_speed,
            player.power_up(PowerUpKind::Speed),
        );

        // Add server reconciliation if we have client position
        if let Ok((client_pos, _, _)) = player_data.get(player.entity) {
            commands.entity(player.entity).insert((
                msg.movement.move_intent, // Never the local player, so we can always overwrite intent
                ServerReconciliation {
                    client_pos: *client_pos,
                    server_pos: msg.movement.pos,
                    server_velocity,
                    correction_progress: 0.0,
                    rtt: rtt.rtt.as_secs_f32(),
                },
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
        let server_velocity = player_movement_velocity(
            msg.movement,
            gameplay_config.player.walk_speed,
            gameplay_config.player.run_speed,
            player.power_up(PowerUpKind::Speed),
        );
        commands.entity(player.entity).insert((
            msg.movement.move_intent,
            CharacterVerticalVelocity(msg.movement.vertical_velocity),
            ServerReconciliation {
                client_pos: *client_pos,
                server_pos: msg.movement.pos,
                server_velocity,
                correction_progress: 0.0,
                rtt: rtt.rtt.as_secs_f32(),
            },
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
    my_player_id: PlayerId,
    msg: SPlayerHit,
) {
    debug!("player {:?} was hit", msg.id);
    if let Some(player) = players.get(&msg.id) {
        commands.entity(player.entity).insert(msg.health);
    }
    if msg.id == my_player_id {
        if let Ok(camera_entity) = camera_query.single() {
            commands.entity(camera_entity).insert(CameraShake {
                timer: Timer::from_seconds(0.3, TimerMode::Once),
                intensity: 3.0,
                dir_x: msg.hit_dir_x,
                // Small vertical companion to the directional hit shake —
                // preserves the prior hardcoded `0.2` vertical bob.
                dir_y: 0.2,
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
pub fn handle_player_death_message(
    commands: &mut Commands,
    players: &mut PlayerMap,
    local_player_info: &mut LocalPlayerInfo,
    feed: &mut GameMessageFeed,
    client_settings: &ClientSettings,
    existing_banners: &Query<Entity, With<HudBannerMarker>>,
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
    }
    if let (Some(killer_id), Some(killer_score)) = (msg.killer, msg.killer_score)
        && let Some(killer_info) = players.get_mut(&killer_id)
    {
        killer_info.score = killer_score;
    }

    if let Some(info) = players.get(&msg.id) {
        commands.entity(info.entity).insert(Health(0.0));
    }

    if msg.id == my_player_id {
        if let Some(info) = players.get(&msg.id) {
            commands.entity(info.entity).insert(Visibility::Hidden);
        }
        local_player_info.is_dead = true;
        // Centered "You have died!" banner. The red full-screen
        // `DeathOverlayMarker` tint and the message-feed `SoloDeath`
        // entry are independent layers; the banner is the headline.
        let banner = &client_settings.hud.banner;
        spawn_hud_banner(
            commands,
            existing_banners,
            &banner.death_text,
            banner.death_duration_secs,
            client_settings.hud.font_sizes.banner,
        );
    } else if let Some(info) = players.remove(&msg.id) {
        commands.entity(info.entity).despawn();
    }
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
                    commands.spawn((
                        AudioPlayer::new(asset_server.load(asset_set.player_sound("collect_power_up").to_owned())),
                        PlaybackSettings::DESPAWN,
                    ));
                }
            }
        }

        player_info.apply_status(&msg);
    }
}

// Player took fall damage. Updates HUD health on the impact frame
// (instead of waiting for the next snapshot) and applies a vertical
// camera shake — same shape as `handle_player_hit_message` but on the
// Y axis only. Unicast, so the message only ever targets the local
// player; no other-player branch.
pub fn handle_fall_damage_message(
    commands: &mut Commands,
    players: &ResMut<PlayerMap>,
    camera_query: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    my_player_id: PlayerId,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    msg: SPlayerFallDamage,
) {
    if let Some(player) = players.get(&msg.id) {
        commands.entity(player.entity).insert(msg.health);
    }
    if msg.id == my_player_id {
        if let Ok(camera_entity) = camera_query.single() {
            commands.entity(camera_entity).insert(CameraShake {
                // Same duration/intensity envelope as a projectile hit, just
                // re-aimed along the vertical axis. `dir_y` is tuned to feel
                // more pronounced than the hit's vertical-companion `0.2` but
                // not jarring — max amplitude ≈ 1.5 vs the hit's 0.6.
                timer: Timer::from_seconds(0.3, TimerMode::Once),
                intensity: 3.0,
                dir_x: 0.0,
                dir_y: 0.5,
                dir_z: 0.0,
                offset_x: 0.0,
                offset_y: 0.0,
                offset_z: 0.0,
            });
        }
        commands.spawn((
            AudioPlayer::new(asset_server.load(asset_set.player_sound("fall_damage").to_owned())),
            PlaybackSettings::DESPAWN,
        ));
    }
}

// Server has assigned the local client a new quest. The player has just
// spawned (this fires at login, right after `SInit`), so kick off the
// announcement banner immediately and remember the text so it can be
// re-shown on every respawn until `SQuestAchieved` retires the entry.
pub fn handle_quest_new_message(
    commands: &mut Commands,
    active_quests: &mut ActiveQuests,
    client_settings: &ClientSettings,
    existing_banners: &Query<Entity, With<HudBannerMarker>>,
    msg: SQuestNew,
) {
    let banner = &client_settings.hud.banner;
    active_quests.pending.insert(msg.id, msg.announcement_text.clone());
    spawn_hud_banner(
        commands,
        existing_banners,
        &msg.announcement_text,
        banner.announcement_duration_secs,
        client_settings.hud.font_sizes.banner,
    );
}

// Server says the local client just completed a quest. Stop showing the
// announcement on future respawns and fire the achieved banner.
pub fn handle_quest_achieved_message(
    commands: &mut Commands,
    active_quests: &mut ActiveQuests,
    client_settings: &ClientSettings,
    existing_banners: &Query<Entity, With<HudBannerMarker>>,
    msg: SQuestAchieved,
) {
    let banner = &client_settings.hud.banner;
    active_quests.pending.remove(&msg.id);
    spawn_hud_banner(
        commands,
        existing_banners,
        &msg.achieved_text,
        banner.achieved_duration_secs,
        client_settings.hud.font_sizes.banner,
    );
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
        }
    }

    #[test]
    fn local_player_death_sets_health_to_zero() {
        use bevy::ecs::system::SystemState;

        let my_id = PlayerId(7);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let entity = app.world_mut().spawn((Health(42.0), Visibility::Visible)).id();
        let world = app.world_mut();
        let mut players = PlayerMap::default();
        players.insert(my_id, player_info(entity, "Alice"));
        let mut local_player_info = LocalPlayerInfo::default();
        let mut feed = GameMessageFeed::default();
        let client_settings = ClientSettings::load_default().expect("load default client settings");
        let mut banner_state: SystemState<Query<Entity, With<crate::ui::HudBannerMarker>>> = SystemState::new(world);
        let banners = banner_state.get(world);
        let mut commands_queue = bevy::ecs::world::CommandQueue::default();

        {
            let mut commands = bevy::ecs::system::Commands::new(&mut commands_queue, world);
            handle_player_death_message(
                &mut commands,
                &mut players,
                &mut local_player_info,
                &mut feed,
                &client_settings,
                &banners,
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
