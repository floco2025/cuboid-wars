use bevy::prelude::*;
use common::protocol::*;

use super::{
    actors::{
        handle_actor_beam_message, handle_actor_death_message, handle_actor_hit_message, handle_actor_move_message,
    },
    context::ServerMessageContext,
    io::apply_pong,
    items::{handle_cookie_collected_message, handle_health_potion_collected_message},
    missiles::{
        handle_missile_detonated_message, handle_missile_launch_message, handle_missile_move_message,
        handle_missiles_collected_message,
    },
    players::{
        handle_player_blast_message, handle_player_death_message, handle_player_fall_damage_message,
        handle_player_hit_message, handle_player_moves_message, handle_player_status_message,
        handle_projectile_shot_message,
    },
    portals::handle_portal_opened_message,
    presentation::{handle_feed_message, handle_firework_message, handle_pressure_plate_message},
    quests::handle_quest_updates_message,
    snapshot::handle_snapshot_message,
};

pub(super) fn route_server_message(
    message: ServerMessage,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    let my_player_id = context.my_player_id.0;

    match message {
        ServerMessage::Init(_) => error!("received Init more than once"),
        ServerMessage::QuestUpdates(message) => handle_quest_updates_message(message, commands, context),
        ServerMessage::Snapshot(message) => {
            handle_snapshot_message(message, commands, my_player_id, context);
        }
        ServerMessage::PlayerMoves(message) => {
            handle_player_moves_message(message, commands, my_player_id, context);
        }
        ServerMessage::ProjectileShot(message) => handle_projectile_shot_message(message, commands, context),
        ServerMessage::ActorMove(message) => handle_actor_move_message(message, commands, context),
        ServerMessage::MissileLaunch(message) => {
            handle_missile_launch_message(message, commands, my_player_id, context);
        }
        ServerMessage::MissileMove(message) => handle_missile_move_message(message, commands, context),
        ServerMessage::PlayerDeath(message) => {
            handle_player_death_message(message, commands, my_player_id, context);
        }
        ServerMessage::ActorDeath(message) => handle_actor_death_message(message, commands, context),
        ServerMessage::MissileDetonated(message) => handle_missile_detonated_message(message, commands, context),
        ServerMessage::PlayerHit(message) => {
            handle_player_hit_message(message, commands, my_player_id, context);
        }
        ServerMessage::PlayerFallDamage(message) => {
            handle_player_fall_damage_message(message, commands, my_player_id, context);
        }
        ServerMessage::PlayerBlast(message) => {
            handle_player_blast_message(message, commands, my_player_id, context);
        }
        ServerMessage::ActorHit(message) => handle_actor_hit_message(message, commands, context),
        ServerMessage::ActorBeam(message) => handle_actor_beam_message(message, commands, context),
        ServerMessage::PlayerStatus(message) => {
            handle_player_status_message(message, commands, my_player_id, context);
        }
        ServerMessage::CookieCollected(message) => {
            handle_cookie_collected_message(message, commands, my_player_id, context);
        }
        ServerMessage::HealthPotionCollected(message) => {
            handle_health_potion_collected_message(message, commands, my_player_id, context);
        }
        ServerMessage::MissilesCollected(message) => {
            handle_missiles_collected_message(message, commands, my_player_id, context);
        }
        ServerMessage::PressurePlate(message) => handle_pressure_plate_message(message, commands, context),
        ServerMessage::Firework(message) => handle_firework_message(message, context),
        ServerMessage::PortalOpened(message) => {
            handle_portal_opened_message(message, commands, my_player_id, context);
        }
        ServerMessage::Feed(message) => handle_feed_message(message, context),
        ServerMessage::Pong(message) => apply_pong(&context.time, &mut context.rtt, message),
    }
}
