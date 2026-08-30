use bevy::{ecs::system::SystemParam, prelude::*};
use common::protocol::*;

use super::{
    actors::{
        ActorMessageContext, handle_actor_beam_message, handle_actor_death_message, handle_actor_hit_message,
        handle_actor_move_message,
    },
    bootstrap::handle_bootstrap_message,
    io::apply_pong,
    items::{ItemMessageContext, handle_cookie_collected_message, handle_health_potion_collected_message},
    missiles::{
        MissileMessageContext, handle_missile_death_message, handle_missile_launch_message,
        handle_missile_move_message, handle_missiles_collected_message,
    },
    players::{
        PlayerMessageContext, handle_player_blast_message, handle_player_death_message,
        handle_player_fall_damage_message, handle_player_hit_message, handle_player_jump_message,
        handle_player_move_message, handle_player_shot_message, handle_player_status_message,
    },
    presentation::{
        PresentationMessageContext, handle_feed_message, handle_firework_message, handle_pressure_plate_message,
    },
    quests::{
        QuestMessageContext, handle_quest_completed_message, handle_quest_progress_message,
        handle_quests_assigned_message,
    },
    snapshot::{SnapshotMessageContext, handle_snapshot_message},
};
use crate::{network::RoundTripTime, players::MyPlayerId};

#[derive(SystemParam)]
pub(super) struct ServerMessageContext<'w, 's> {
    barrier_kind_table: Res<'w, BarrierKindTable>,
    my_player_id: Option<Res<'w, MyPlayerId>>,
    time: Res<'w, Time>,
    rtt: ResMut<'w, RoundTripTime>,
    handlers: ParamSet<
        'w,
        's,
        (
            SnapshotMessageContext<'w, 's>,
            PlayerMessageContext<'w, 's>,
            ActorMessageContext<'w, 's>,
            ItemMessageContext<'w>,
            MissileMessageContext<'w, 's>,
            PresentationMessageContext<'w>,
            QuestMessageContext<'w>,
        ),
    >,
}

pub(super) fn route_server_message(
    message: ServerMessage,
    commands: &mut Commands,
    context: &mut ServerMessageContext,
) {
    let my_player_id = context.my_player_id.as_deref().map(|id| id.0);
    let initialized = my_player_id.is_some();

    match message {
        ServerMessage::Init(_) if initialized => error!("received Init more than once"),
        ServerMessage::Init(message) => {
            handle_bootstrap_message(&message, commands, &context.barrier_kind_table);
        }
        // Quest state has no snapshot fallback and does not depend on bootstrap.
        ServerMessage::QuestsAssigned(message) => {
            handle_quests_assigned_message(&message, &mut context.handlers.p6());
        }
        ServerMessage::QuestProgress(message) => {
            handle_quest_progress_message(&message, &mut context.handlers.p6());
        }
        ServerMessage::QuestCompleted(message) => {
            handle_quest_completed_message(&message, commands, initialized, &mut context.handlers.p6());
        }
        // Bootstrap resources are deferred; the next snapshot restores durable world state.
        _ if !initialized => {}
        ServerMessage::Snapshot(message) => {
            handle_snapshot_message(
                &message,
                commands,
                my_player_id.expect("initialized client missing player id"),
                &context.rtt,
                &mut context.handlers.p0(),
            );
        }
        ServerMessage::PlayerMove(message) => {
            handle_player_move_message(&message, commands, &context.rtt, &mut context.handlers.p1());
        }
        ServerMessage::PlayerJump(message) => {
            handle_player_jump_message(&message, commands, &context.rtt, &mut context.handlers.p1());
        }
        ServerMessage::PlayerShot(message) => {
            handle_player_shot_message(&message, commands, &mut context.handlers.p1());
        }
        ServerMessage::ActorMove(message) => {
            handle_actor_move_message(&message, commands, &context.rtt, &mut context.handlers.p2());
        }
        ServerMessage::MissileLaunch(message) => {
            handle_missile_launch_message(
                &message,
                commands,
                my_player_id.expect("initialized client missing player id"),
                &mut context.handlers.p4(),
            );
        }
        ServerMessage::MissileMove(message) => {
            handle_missile_move_message(&message, commands, &context.rtt, &mut context.handlers.p4());
        }
        ServerMessage::PlayerDeath(message) => {
            handle_player_death_message(
                &message,
                commands,
                my_player_id.expect("initialized client missing player id"),
                &mut context.handlers.p1(),
            );
        }
        ServerMessage::ActorDeath(message) => {
            handle_actor_death_message(&message, commands, &mut context.handlers.p2());
        }
        ServerMessage::MissileDeath(message) => {
            handle_missile_death_message(&message, commands, &mut context.handlers.p4());
        }
        ServerMessage::PlayerHit(message) => {
            handle_player_hit_message(
                &message,
                commands,
                my_player_id.expect("initialized client missing player id"),
                &mut context.handlers.p1(),
            );
        }
        ServerMessage::PlayerFallDamage(message) => {
            handle_player_fall_damage_message(
                &message,
                commands,
                my_player_id.expect("initialized client missing player id"),
                &mut context.handlers.p1(),
            );
        }
        ServerMessage::PlayerBlast(message) => {
            handle_player_blast_message(
                &message,
                commands,
                my_player_id.expect("initialized client missing player id"),
                &mut context.handlers.p1(),
            );
        }
        ServerMessage::ActorHit(message) => {
            handle_actor_hit_message(&message, commands, &mut context.handlers.p2());
        }
        ServerMessage::ActorBeam(message) => {
            handle_actor_beam_message(&message, commands, &mut context.handlers.p2());
        }
        ServerMessage::PlayerStatus(message) => {
            handle_player_status_message(
                &message,
                commands,
                my_player_id.expect("initialized client missing player id"),
                &mut context.handlers.p1(),
            );
        }
        ServerMessage::CookieCollected(message) => {
            handle_cookie_collected_message(
                &message,
                commands,
                my_player_id.expect("initialized client missing player id"),
                &mut context.handlers.p3(),
            );
        }
        ServerMessage::HealthPotionCollected(message) => {
            handle_health_potion_collected_message(
                &message,
                commands,
                my_player_id.expect("initialized client missing player id"),
                &mut context.handlers.p3(),
            );
        }
        ServerMessage::MissilesCollected(message) => {
            handle_missiles_collected_message(
                &message,
                commands,
                my_player_id.expect("initialized client missing player id"),
                &mut context.handlers.p4(),
            );
        }
        ServerMessage::PressurePlate(message) => {
            handle_pressure_plate_message(&message, commands, &mut context.handlers.p5());
        }
        ServerMessage::Firework(message) => {
            handle_firework_message(&message, &mut context.handlers.p5());
        }
        ServerMessage::Feed(message) => {
            handle_feed_message(&message, &mut context.handlers.p5());
        }
        ServerMessage::Pong(message) => {
            apply_pong(&context.time, &mut context.rtt, &message);
        }
    }
}
