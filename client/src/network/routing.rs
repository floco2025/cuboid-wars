use bevy::prelude::*;
use common::protocol::*;

use super::{
    actors::{
        handle_actor_beam_message, handle_actor_death_message, handle_actor_hit_message, handle_actor_move_message,
    },
    bootstrap::handle_bootstrap_message,
    context::ServerMessageContext,
    io::apply_pong,
    items::{handle_cookie_collected_message, handle_health_potion_collected_message},
    missiles::{
        handle_missile_death_message, handle_missile_launch_message, handle_missile_move_message,
        handle_missiles_collected_message,
    },
    players::{
        handle_player_blast_message, handle_player_death_message, handle_player_fall_damage_message,
        handle_player_hit_message, handle_player_jump_message, handle_player_move_message, handle_player_shot_message,
        handle_player_status_message,
    },
    presentation::{handle_feed_message, handle_firework_message, handle_pressure_plate_message},
    quests::{handle_quest_completed_message, handle_quest_progress_message, handle_quests_assigned_message},
    snapshot::handle_snapshot_message,
};

// `MyPlayerId` lands with the other bootstrap resources, so its presence
// gates everything that needs them. QUIC keeps order only within a stream
// (see `common::network`), so a message can arrive ahead of `SInit`: the
// next snapshot restores world state, and one-shot cues are ephemeral.
pub(super) fn route_server_message(
    message: ServerMessage,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    let my_player_id = context.my_player_id.as_deref().map(|id| id.0);

    match (my_player_id, message) {
        // Quest state has no snapshot fallback and does not depend on bootstrap.
        (_, ServerMessage::QuestsAssigned(message)) => handle_quests_assigned_message(message, context),
        (_, ServerMessage::QuestProgress(message)) => handle_quest_progress_message(message, context),
        (_, ServerMessage::QuestCompleted(message)) => handle_quest_completed_message(message, commands, context),
        (None, ServerMessage::Init(message)) => {
            handle_bootstrap_message(message, commands, &context.barrier_kind_table);
        }
        (Some(_), ServerMessage::Init(_)) => error!("received Init more than once"),
        (None, _) => debug!("dropped a server message received before Init"),
        (Some(my_player_id), ServerMessage::Snapshot(message)) => {
            handle_snapshot_message(message, commands, my_player_id, context);
        }
        (Some(_), ServerMessage::PlayerMove(message)) => handle_player_move_message(message, commands, context),
        (Some(_), ServerMessage::PlayerJump(message)) => handle_player_jump_message(message, commands, context),
        (Some(_), ServerMessage::PlayerShot(message)) => handle_player_shot_message(message, commands, context),
        (Some(_), ServerMessage::ActorMove(message)) => handle_actor_move_message(message, commands, context),
        (Some(my_player_id), ServerMessage::MissileLaunch(message)) => {
            handle_missile_launch_message(message, commands, my_player_id, context);
        }
        (Some(_), ServerMessage::MissileMove(message)) => handle_missile_move_message(message, commands, context),
        (Some(my_player_id), ServerMessage::PlayerDeath(message)) => {
            handle_player_death_message(message, commands, my_player_id, context);
        }
        (Some(_), ServerMessage::ActorDeath(message)) => handle_actor_death_message(message, commands, context),
        (Some(_), ServerMessage::MissileDeath(message)) => handle_missile_death_message(message, commands, context),
        (Some(my_player_id), ServerMessage::PlayerHit(message)) => {
            handle_player_hit_message(message, commands, my_player_id, context);
        }
        (Some(my_player_id), ServerMessage::PlayerFallDamage(message)) => {
            handle_player_fall_damage_message(message, commands, my_player_id, context);
        }
        (Some(my_player_id), ServerMessage::PlayerBlast(message)) => {
            handle_player_blast_message(message, commands, my_player_id, context);
        }
        (Some(_), ServerMessage::ActorHit(message)) => handle_actor_hit_message(message, commands, context),
        (Some(_), ServerMessage::ActorBeam(message)) => handle_actor_beam_message(message, commands, context),
        (Some(my_player_id), ServerMessage::PlayerStatus(message)) => {
            handle_player_status_message(message, commands, my_player_id, context);
        }
        (Some(my_player_id), ServerMessage::CookieCollected(message)) => {
            handle_cookie_collected_message(message, commands, my_player_id, context);
        }
        (Some(my_player_id), ServerMessage::HealthPotionCollected(message)) => {
            handle_health_potion_collected_message(message, commands, my_player_id, context);
        }
        (Some(my_player_id), ServerMessage::MissilesCollected(message)) => {
            handle_missiles_collected_message(message, commands, my_player_id, context);
        }
        (Some(_), ServerMessage::PressurePlate(message)) => handle_pressure_plate_message(message, commands, context),
        (Some(_), ServerMessage::Firework(message)) => handle_firework_message(message, context),
        (Some(_), ServerMessage::Feed(message)) => handle_feed_message(message, context),
        (Some(_), ServerMessage::Pong(message)) => apply_pong(&context.time, &mut context.rtt, message),
    }
}
