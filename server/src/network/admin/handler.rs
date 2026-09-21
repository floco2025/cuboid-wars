use bevy::{ecs::system::SystemParam, prelude::*};

use super::execute::{AdminOutcome, run_admin_command};
use crate::{
    actors::{ActorMap, ActorSpawner, PendingActorSpawns},
    combat::PendingExplosions,
    config::{PowerUpsConfig, ServerGameplayConfig},
    map::WeatherState,
    network::{FeedAudience, FeedEvent, SharedWorld, emit_feed, handlers::CharacterQueries},
    players::{Invincibility, PlayerInfo, PlayerMap},
    portals::PortalAssignments,
    quests::{QuestBoard, QuestCatalog},
};
use common::{
    celestial::CelestialClockAnchor,
    protocol::{CAdmin, FieldTable, MapItems, PlayerId, ServerTick},
};

fn admin_authorized(_info: &PlayerInfo) -> bool {
    true
}

// Bundled so the routing system stays under Bevy's system-parameter limit.
#[derive(SystemParam)]
pub struct AdminContext<'w> {
    pub weather: ResMut<'w, WeatherState>,
    pub celestial_clock: ResMut<'w, CelestialClockAnchor>,
    pub pending_explosions: ResMut<'w, PendingExplosions>,
    pub invincibility: ResMut<'w, Invincibility>,
    pub actor_spawner: ResMut<'w, ActorSpawner>,
    pub server_gameplay_config: Res<'w, ServerGameplayConfig>,
    pub power_ups: Res<'w, PowerUpsConfig>,
    pub field_table: Res<'w, FieldTable>,
    pub map_items: Res<'w, MapItems>,
    pub quest_catalog: Res<'w, QuestCatalog>,
    pub server_tick: Res<'w, ServerTick>,
}

pub fn handle_admin_message(
    commands: &mut Commands,
    players: &mut PlayerMap,
    actors: &mut ActorMap,
    id: PlayerId,
    admin: &mut AdminContext,
    queries: &mut CharacterQueries,
    world: &SharedWorld,
    portal_assignments: &PortalAssignments,
    pending_actor_spawns: &mut PendingActorSpawns,
    quest_board: &mut QuestBoard,
    msg: &CAdmin,
) {
    let Some(info) = players.get(&id) else {
        return;
    };
    let outcome = if admin_authorized(info) {
        run_admin_command(
            commands,
            players,
            actors,
            id,
            admin,
            queries,
            world,
            portal_assignments,
            pending_actor_spawns,
            quest_board,
            &msg.command,
        )
    } else {
        AdminOutcome::Private("not authorized".to_owned())
    };
    let feed = &admin.server_gameplay_config.feed;
    match outcome {
        AdminOutcome::Public(text) if feed.admin_action => emit_feed(
            players,
            feed,
            FeedAudience::Everyone,
            FeedEvent::AdminAction {
                name: players.display_name(&id),
                text,
            },
        ),
        // A disabled public announcement must not hide the outcome from its issuer.
        AdminOutcome::Public(text) | AdminOutcome::Private(text) => {
            emit_feed(players, feed, FeedAudience::Player(id), FeedEvent::AdminReply { text });
        }
    }
}
