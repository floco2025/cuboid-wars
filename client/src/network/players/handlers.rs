use bevy::prelude::*;

use crate::{
    cameras::MainCameraMarker,
    characters::PreviousTickPosition,
    config::{AssetSet, ClientSettings},
    network::{RoundTripTime, ServerReconciliation},
    players::{CameraShake, CuboidShake, LocalPlayerInfo, PlayerMap},
    projectiles::{ProjectileAssets, spawn_projectiles},
    ui::{GameMessage, GameMessageFeed, PendingBanner, QuestEntry, QuestLog},
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
        let server_velocity = player_movement_velocity(
            msg.movement,
            gameplay_config.player.walk_speed,
            gameplay_config.player.run_speed,
            player.power_up(PowerUpKind::Speed),
        );
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
        let banner = &client_settings.hud.banner;
        pending_banner.set(banner.death_text.clone(), banner.death_duration_secs);
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

// Server assigned the local client a batch of quests (at login right after
// `SInit`, or in-game from a quest-giver). Seed each new quest into the panel's
// log and show ONE combined announcement banner — title + description per
// quest. Already-known ids are skipped (defensive; the server only sends
// genuinely-new quests).
pub fn handle_quests_assigned_message(
    quest_log: &mut QuestLog,
    client_settings: &ClientSettings,
    pending_banner: &mut PendingBanner,
    msg: SQuestsAssigned,
) {
    let mut lines = Vec::new();
    for quest in msg.quests {
        if quest_log.entries.contains_key(&quest.id) {
            continue;
        }
        lines.push(format!("{}: {}", quest.title, quest.description));
        quest_log.entries.insert(
            quest.id,
            QuestEntry {
                title: quest.title,
                description: quest.description,
                progress: quest.progress,
                threshold: quest.threshold,
                completed: false,
                order: quest.order,
            },
        );
    }
    if lines.is_empty() {
        return;
    }
    pending_banner.set(
        lines.join("\n"),
        client_settings.hud.banner.quest_announcement_duration_secs,
    );
}

// A quest's progress advanced. Carries the absolute value, so keep the max to
// ignore a reordered/stale update. A progress message for an unknown id (e.g.
// arriving before its assignment batch) is ignored — the assignment seeds it.
pub fn handle_quest_progress_message(quest_log: &mut QuestLog, msg: SQuestProgress) {
    if let Some(entry) = quest_log.entries.get_mut(&msg.id) {
        entry.progress = entry.progress.max(msg.progress);
    }
}

// Server says the local client just completed a quest. Mark it done in the
// panel (kept, shown completed), fire the completion banner, and play the win
// sound.
pub fn handle_quest_completed_message(
    commands: &mut Commands,
    quest_log: &mut QuestLog,
    client_settings: &ClientSettings,
    pending_banner: &mut PendingBanner,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    msg: SQuestCompleted,
) {
    if let Some(entry) = quest_log.entries.get_mut(&msg.id) {
        entry.progress = entry.threshold;
        entry.completed = true;
    }
    pending_banner.set(
        msg.completed_text,
        client_settings.hud.banner.quest_completed_duration_secs,
    );
    commands.spawn((
        AudioPlayer::new(asset_server.load(asset_set.player_sound("quest_completed").to_owned())),
        PlaybackSettings::DESPAWN,
    ));
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

    fn quest_log_with(id: &str, progress: u32, threshold: u32) -> QuestLog {
        let mut log = QuestLog::default();
        log.entries.insert(
            QuestId(id.to_owned()),
            QuestEntry {
                title: "Gold".to_owned(),
                description: "collect gold".to_owned(),
                progress,
                threshold,
                completed: false,
                order: 0,
            },
        );
        log
    }

    #[test]
    fn quest_progress_keeps_max_and_ignores_unknown_id() {
        let mut log = quest_log_with("collect_gold", 3, 10);

        // Advancing update applies.
        handle_quest_progress_message(
            &mut log,
            SQuestProgress {
                id: QuestId("collect_gold".to_owned()),
                progress: 4,
            },
        );
        assert_eq!(log.entries[&QuestId("collect_gold".to_owned())].progress, 4);

        // A stale, lower value is discarded (absolute value + max guard).
        handle_quest_progress_message(
            &mut log,
            SQuestProgress {
                id: QuestId("collect_gold".to_owned()),
                progress: 2,
            },
        );
        assert_eq!(log.entries[&QuestId("collect_gold".to_owned())].progress, 4);

        // An update for an unknown quest is a no-op (doesn't insert).
        handle_quest_progress_message(
            &mut log,
            SQuestProgress {
                id: QuestId("unknown".to_owned()),
                progress: 9,
            },
        );
        assert_eq!(log.entries.len(), 1);
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
        let mut commands_queue = bevy::ecs::world::CommandQueue::default();

        {
            let mut commands = bevy::ecs::system::Commands::new(&mut commands_queue, world);
            handle_player_death_message(
                &mut commands,
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
