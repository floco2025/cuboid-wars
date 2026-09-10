use bevy::{ecs::system::SystemParam, prelude::*};

use super::{
    admin::{AdminContext, handle_admin_message},
    handlers::{CharacterQueries, SharedWorld, handle_chat_message, handle_ping_message},
    login::handle_login_message,
};
use crate::{
    actors::{ActorMap, PendingActorSpawns},
    missiles::{MissileMap, handle_missile_detonated, handle_missile_moves, handle_missile_shot_message},
    network::ServerToClient,
    players::{PlayerMap, handle_move_outcome, queue_player_movement},
    portals::{PortalAssignments, PortalMap, handle_portal_shot_message},
    projectiles::{PendingProjectileHits, handle_projectile_shot_message},
    quests::{QuestBoard, QuestCatalog},
};
use common::{physics::PortalSet, protocol::*};

#[derive(SystemParam)]
pub(super) struct ClientMessageContext<'w, 's> {
    pub(super) players: ResMut<'w, PlayerMap>,
    time: Res<'w, Time>,
    pub(super) world: SharedWorld<'w>,
    queries: CharacterQueries<'w, 's>,
    actors: ResMut<'w, ActorMap>,
    pending_projectile_hits: ResMut<'w, PendingProjectileHits>,
    pub(super) missiles: ResMut<'w, MissileMap>,
    pending_actor_spawns: ResMut<'w, PendingActorSpawns>,
    pub(super) portals: ResMut<'w, PortalMap>,
    pub(super) portal_assignments: ResMut<'w, PortalAssignments>,
    pub(super) portal_set: ResMut<'w, PortalSet>,
    admin: AdminContext<'w>,
    pub(super) quest_board: ResMut<'w, QuestBoard>,
    pub(super) quest_catalog: Res<'w, QuestCatalog>,
}

pub(super) fn route_client_message(
    commands: &mut Commands,
    id: PlayerId,
    message: ClientMessage,
    context: &mut ClientMessageContext,
) {
    let Some(player) = context.players.get(&id) else {
        error!("received message for unknown player#{}", id.0);
        return;
    };
    let logged_in = player.connection.logged_in;
    // `None` while dead: the body-bound arms drop the message until respawn,
    // while Ping/Admin/Chat keep the console and RTT working meanwhile.
    let entity = player.entity();

    match message {
        ClientMessage::Login(message) if !logged_in => {
            let Some(entity) = entity else {
                error!("player#{} reached login without an entity", id.0);
                return;
            };
            handle_login_message(
                commands,
                entity,
                id,
                message,
                &mut context.players,
                &context.world,
                &context.queries,
                &context.quest_catalog,
                &context.quest_board,
                &mut context.portal_assignments,
                &mut context.portals,
                &mut context.portal_set,
            );
        }
        ClientMessage::Login(_) => {
            warn!("{} sent a second login", context.players.describe(&id));
            // Close to enforce a single-login flow.
            if let Some(player) = context.players.get(&id) {
                let _ = player.connection.channel.send(ServerToClient::Close);
            }
        }
        // An unreliable message can overtake `CLogin`; drop it.
        _ if !logged_in => {
            warn!("{} sent gameplay traffic before login", context.players.describe(&id));
        }
        ClientMessage::Move(message) => {
            if entity.is_none() {
                return;
            }
            trace!("{:?} input: {:?}", id, message);
            queue_player_movement(id, message, &mut context.players, &context.world.carriers);
        }
        ClientMessage::MoveOutcome(message) => handle_move_outcome(id, message, &mut context.players),
        ClientMessage::ProjectileShot(message) => handle_projectile_shot_message(
            id,
            message,
            &context.players,
            &context.world.gameplay_config.projectiles,
        ),
        ClientMessage::ProjectileHit(message) => context.pending_projectile_hits.push(id, message),
        ClientMessage::MissileShot(message) => handle_missile_shot_message(
            id,
            message,
            &mut context.players,
            &mut context.missiles,
            *context.world.tick,
        ),
        ClientMessage::MissileMoves(message) => {
            handle_missile_moves(id, message, &mut context.missiles, &context.players)
        }
        ClientMessage::MissileDetonated(message) => handle_missile_detonated(
            id,
            message,
            &mut context.missiles,
            &context.players,
            &mut context.admin.pending_explosions,
            *context.world.tick,
        ),
        ClientMessage::PortalShot(message) => {
            if entity.is_none() {
                return;
            }
            debug!("{} portal shot ({:?})", context.players.describe(&id), message.result);
            handle_portal_shot_message(
                id,
                &message,
                &mut context.players,
                &context.time,
                &context.world,
                &context.portal_assignments,
                &mut context.portals,
                &mut context.portal_set,
            );
        }
        ClientMessage::Ping(message) => {
            trace!("{:?} ping: {:?}", id, message);
            handle_ping_message(id, message, &context.players, *context.world.tick);
        }
        ClientMessage::Admin(message) => {
            debug!("{} admin command: {:?}", context.players.describe(&id), message.command);
            handle_admin_message(
                commands,
                &mut context.players,
                &mut context.actors,
                id,
                &mut context.admin,
                &context.queries.player_data,
                &context.world,
                &mut context.pending_actor_spawns,
                &mut context.quest_board,
                &message,
            );
        }
        ClientMessage::Chat(message) => handle_chat_message(
            id,
            &message,
            &context.players,
            &context.world.server_gameplay_config.feed,
        ),
    }
}
