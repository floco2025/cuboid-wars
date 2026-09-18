use bevy::prelude::*;

use super::{
    command::{AdminCommand, CelestialCommand, HELP_TEXT, PlayerTarget, parse_admin_command},
    handler::AdminContext,
};
use crate::{
    actors::{ActorMap, PendingActorSpawns, expedite_actor_respawns},
    combat::{DeathSource, kill_player},
    config::{PowerUpMode, ServerGameplayConfig},
    network::{SharedWorld, broadcast_firework_show, broadcast_to_all, handlers::CharacterQueries},
    players::{
        PlayerMap, PlayerStateQuery, checkpoint_numbered, occupied_player_positions, place_player_body,
        player_spawn_destination,
    },
    portals::PortalAssignments,
    quests::{QuestBoard, QuestCatalog, complete_quest, unlock_quest},
};
use common::{
    celestial::{CelestialClockAnchor, CelestialCycleSettings, LocalTime},
    protocol::{
        BarrierKindId, Health, ItemType, PlayerId, PowerUpKind, QuestGroupProgress, QuestId, QuestScope, SPlayerStatus,
        ServerMessage,
    },
};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum AdminOutcome {
    Private(String),
    Public(String),
}

pub(super) fn run_admin_command(
    commands: &mut Commands,
    players: &mut PlayerMap,
    actors: &mut ActorMap,
    sender: PlayerId,
    admin: &mut AdminContext,
    queries: &mut CharacterQueries,
    world: &SharedWorld,
    portal_assignments: &PortalAssignments,
    pending_actor_spawns: &mut PendingActorSpawns,
    quest_board: &mut QuestBoard,
    command: &str,
) -> AdminOutcome {
    use AdminOutcome::{Private, Public};

    let parsed = parse_admin_command(command);
    let tick = admin.server_tick.0;
    let server_hz = admin.server_gameplay_config.network.server_hz;
    let cycle = admin.server_gameplay_config.cycles.celestial;

    match parsed {
        AdminCommand::Help => Private(HELP_TEXT.to_owned()),
        AdminCommand::NotACommand => Private("not a command (commands start with /)".to_owned()),
        AdminCommand::Unknown => Private(format!("unknown command {command:?} (try /help)")),
        AdminCommand::MissingTarget(verb) => Private(if verb == "kill" {
            format!("usage: /{verb} <name> or /{verb} @a")
        } else {
            format!("usage: /{verb} <name>")
        }),
        AdminCommand::WeatherRain => match admin.weather.hold_rain() {
            Ok(()) => Public("weather set to rain".to_owned()),
            Err(reason) => Private(reason.to_owned()),
        },
        AdminCommand::WeatherClear => match admin.weather.hold_clear() {
            Ok(()) => Public("weather set to clear".to_owned()),
            Err(reason) => Private(reason.to_owned()),
        },
        AdminCommand::WeatherAuto => match admin.weather.resume_auto() {
            Ok(()) => Public("weather cycle resumed".to_owned()),
            Err(reason) => Private(reason.to_owned()),
        },
        AdminCommand::WeatherStatus => Private(admin.weather.status()),
        AdminCommand::Celestial(command) => {
            run_celestial_command(command, &mut admin.celestial_clock, tick, server_hz, cycle)
        }
        AdminCommand::God(explicit) => {
            let enabled = explicit.unwrap_or(!admin.invincibility.0);
            admin.invincibility.0 = enabled;
            Public(format!("god mode {}", if enabled { "on" } else { "off" }))
        }
        AdminCommand::Peace(explicit) => {
            let enabled = explicit.unwrap_or(!actors.peaceful);
            actors.set_peaceful(enabled);
            Public(if enabled {
                "peace mode on: actors ignore players and cannot attack them".to_owned()
            } else {
                "peace mode off: actor attacks resumed".to_owned()
            })
        }
        AdminCommand::KillAllPlayers => {
            let targets = alive_players(players, None);
            let count = kill_targets(commands, players, admin, &queries.player_data.as_readonly(), &targets);
            Public(format!("killed {count} player(s)"))
        }
        AdminCommand::KillPlayer(name) => {
            let targets = alive_players(players, Some(&name));
            if targets.is_empty() {
                return Private(format!("unknown player {name:?}"));
            }
            let count = kill_targets(commands, players, admin, &queries.player_data.as_readonly(), &targets);
            Public(format!("killed {count} player(s)"))
        }
        AdminCommand::KillActors(kind) => {
            if let Some(error) = actor_kind_error(kind.as_deref(), &admin.server_gameplay_config) {
                return Private(error);
            }
            let mut count = 0usize;
            for (_, info) in actors.iter() {
                if kind.as_deref().is_none_or(|kind| info.spawn_kind == kind) {
                    commands.entity(info.entity).insert(Health(0.0));
                    count += 1;
                }
            }
            Public(format!("killed {count} actor(s)"))
        }
        AdminCommand::RespawnActors(kind) => {
            if let Some(error) = actor_kind_error(kind.as_deref(), &admin.server_gameplay_config) {
                return Private(error);
            }
            let count = expedite_actor_respawns(
                actors,
                pending_actor_spawns,
                &mut admin.actor_spawner,
                &world.map_config,
                players.logged_in_count(),
                admin.server_tick.0,
                kind.as_deref(),
            );
            Public(format!("respawning {count} actor(s)"))
        }
        AdminCommand::Heal(target) => {
            let targets = match &target {
                PlayerTarget::Sender => alive_players(players, None)
                    .into_iter()
                    .filter(|(id, _)| *id == sender)
                    .collect(),
                PlayerTarget::All => alive_players(players, None),
                PlayerTarget::Named(name) => {
                    let targets = alive_players(players, Some(name));
                    if targets.is_empty() {
                        return Private(format!("unknown player {name:?}"));
                    }
                    targets
                }
            };
            let max_health = admin.server_gameplay_config.combat.health.player.max;
            for (_, entity) in &targets {
                if let Ok((_, _, mut health)) = queries.player_data.get_mut(*entity) {
                    *health = Health(max_health);
                } else {
                    // A login in this batch may still be waiting for its body components.
                    commands.entity(*entity).insert(Health(max_health));
                }
            }
            let text = format!("healed {} player(s)", targets.len());
            if targets.iter().any(|(id, _)| *id != sender) {
                Public(text)
            } else {
                Private(text)
            }
        }
        AdminCommand::CheckpointStatus => {
            let Some(info) = players.get(&sender) else {
                return Private("sender not found".to_owned());
            };
            Private(format!("checkpoint: {}", info.session.checkpoint.number))
        }
        AdminCommand::SetCheckpoint(number) => {
            let saved = match checkpoint_numbered(&world.map_layout.checkpoints, number) {
                Ok(saved) => saved,
                Err(message) => return Private(message),
            };
            let Some(info) = players.get_mut(&sender) else {
                return Private("sender not found".to_owned());
            };
            info.session.checkpoint = saved;
            Private(format!("checkpoint set to {number}"))
        }
        AdminCommand::CheckpointUsage => Private("usage: /checkpoint [number]".to_owned()),
        // A relocation like the void rescue: the living body moves to its
        // saved checkpoint with its health, equipment, and score intact.
        AdminCommand::Return => {
            let Some(info) = players.get(&sender) else {
                return Private("sender not found".to_owned());
            };
            let Some(entity) = info.entity() else {
                return Private("dead: the respawn returns you".to_owned());
            };
            let Ok((_, _, health)) = queries.player_data.get(entity) else {
                return Private("sender has no body".to_owned());
            };
            let health = *health;
            let saved = info.session.checkpoint;
            let occupied = occupied_player_positions(players, &world.carriers, sender);
            let Some(spawn) = player_spawn_destination(
                &world.map_config,
                &world.map_layout.checkpoints,
                &world.carriers,
                &world.collision_world,
                &occupied,
                world.gameplay_config.player.physics(),
                saved,
            ) else {
                return Private(format!("checkpoint {} is blocked", saved.number));
            };
            if let Some(info) = players.get_mut(&sender) {
                info.advance_body();
            }
            place_player_body(
                commands,
                players,
                sender,
                entity,
                &spawn,
                health,
                tick,
                portal_assignments.get(&sender),
            );
            Private(format!("returned to checkpoint {}", saved.number))
        }
        AdminCommand::GiveKeys => {
            let Some(info) = players.get_mut(&sender) else {
                return Private("sender not found".to_owned());
            };
            let mut added = 0usize;
            for index in 0..admin.key_kind_table.len() {
                if let Ok(kind) = u16::try_from(index)
                    && info.add_key(BarrierKindId(kind))
                {
                    added += 1;
                }
            }
            let status = info.status(sender);
            broadcast_to_all(players, ServerMessage::PlayerStatus(status));
            Private(format!("gave {added} key(s)"))
        }
        AdminCommand::GiveKey(color) => match admin.key_kind_table.index_of(&color) {
            Some(kind) => {
                let Some(info) = players.get_mut(&sender) else {
                    return Private("sender not found".to_owned());
                };
                if !info.add_key(kind) {
                    return Private(format!("already holding the {color} key"));
                }
                let status = SPlayerStatus {
                    collected: Some(ItemType::Key(kind)),
                    ..info.status(sender)
                };
                broadcast_to_all(players, ServerMessage::PlayerStatus(status));
                Private(format!("gave the {color} key"))
            }
            None => Private(format!(
                "unknown key color {color:?} (colors: {})",
                admin.key_kind_table.ids().join(", ")
            )),
        },
        AdminCommand::GivePowerups => {
            let Some(info) = players.get_mut(&sender) else {
                return Private("sender not found".to_owned());
            };
            for item_type in PowerUpKind::ALL.map(PowerUpKind::to_item_type) {
                info.grant_power_up(item_type, &admin.power_ups);
            }
            let status = info.status(sender);
            broadcast_to_all(players, ServerMessage::PlayerStatus(status));
            Private(format!("gave {} power-ups", PowerUpKind::COUNT))
        }
        AdminCommand::GivePowerup(power_up) => {
            let power_up_ids = PowerUpKind::ALL.map(|kind| kind.to_item_type().config_id());
            let Some(kind) = PowerUpKind::ALL
                .into_iter()
                .find(|kind| kind.to_item_type().config_id() == power_up)
            else {
                return Private(format!(
                    "unknown power-up {power_up:?} (power-ups: {})",
                    power_up_ids.join(", ")
                ));
            };
            let Some(info) = players.get_mut(&sender) else {
                return Private("sender not found".to_owned());
            };
            if matches!(admin.power_ups.mode(kind), PowerUpMode::Always {}) {
                return Private(format!("{power_up} is always active on this map"));
            }
            let item_type = kind.to_item_type();
            info.grant_power_up(item_type, &admin.power_ups);
            let status = SPlayerStatus {
                collected: Some(item_type),
                ..info.status(sender)
            };
            broadcast_to_all(players, ServerMessage::PlayerStatus(status));
            Private(format!("gave the {power_up} power-up"))
        }
        AdminCommand::GiveMissiles => {
            let Some(info) = players.get_mut(&sender) else {
                return Private("sender not found".to_owned());
            };
            let max = world.gameplay_config.missiles.max_missiles;
            let missiles = info.add_missiles(max, max);
            let status = SPlayerStatus {
                collected: Some(ItemType::MissilePack),
                ..info.status(sender)
            };
            broadcast_to_all(players, ServerMessage::PlayerStatus(status));
            Private(format!("gave missiles ({missiles}/{max})"))
        }
        AdminCommand::Firework => {
            broadcast_firework_show(players);
            Public("launched fireworks".to_owned())
        }
        AdminCommand::QuestStatus => Private(quest_status(players, quest_board, &admin.quest_catalog, sender)),
        AdminCommand::CompleteQuest(id, target) => {
            let config = &admin.server_gameplay_config;
            let catalog = &admin.quest_catalog;
            let quest_id = QuestId(id.clone());
            let Some(quest) = catalog.get(&quest_id) else {
                let ids: Vec<&str> = catalog.iter().map(|quest| quest.id.0.as_str()).collect();
                return Private(format!("unknown quest {id:?} (quests: {})", ids.join(", ")));
            };
            let title = &quest.title;
            if quest_board.is_completed(&quest_id) {
                return Private(format!("{title} is already completed"));
            }
            let targets = match &target {
                PlayerTarget::Sender => vec![sender],
                PlayerTarget::All => active_players(players, None),
                PlayerTarget::Named(name) => {
                    let targets = active_players(players, Some(name));
                    if targets.is_empty() {
                        return Private(format!("unknown player {name:?}"));
                    }
                    targets
                }
            };
            unlock_quest(players, quest_board, catalog, &quest_id);
            let finished = complete_quest(players, quest_board, catalog, &config.feed, quest, &targets);
            match quest.scope {
                QuestScope::Individual if finished == 0 => {
                    Private(format!("{title} is already completed for those players"))
                }
                QuestScope::Individual => {
                    let text = format!("completed {title} for {finished} player(s)");
                    if targets.iter().any(|id| *id != sender) {
                        Public(text)
                    } else {
                        Private(text)
                    }
                }
                QuestScope::Everyone if !quest_board.is_completed(&quest_id) => {
                    Public(format!("finished {title} for {finished} player(s)"))
                }
                QuestScope::Everyone | QuestScope::Shared => Public(format!("completed {title}")),
            }
        }
        AdminCommand::Kick(name) => {
            let mut count = 0usize;
            for (_, info) in players.iter_mut() {
                if info.connection.logged_in && info.connection.name.to_lowercase() == name.to_lowercase() {
                    info.connection.hang_up();
                    count += 1;
                }
            }
            if count == 0 {
                Private(format!("unknown player {name:?}"))
            } else {
                Public(format!("kicked {count} player(s)"))
            }
        }
    }
}

fn run_celestial_command(
    command: CelestialCommand,
    clock: &mut CelestialClockAnchor,
    tick: u32,
    server_hz: u32,
    cycle: CelestialCycleSettings,
) -> AdminOutcome {
    use AdminOutcome::{Private, Public};

    match command {
        CelestialCommand::TimeSeek(time) => {
            clock.seek_time(time, tick, server_hz, cycle);
            Public(format!("time set to {} (held)", time.format()))
        }
        CelestialCommand::TimeAuto => {
            if clock.resume(tick, server_hz, cycle) {
                Public("time resumed".to_owned())
            } else {
                Private("time already running".to_owned())
            }
        }
        CelestialCommand::TimeStatus => Private(time_status(clock, tick, server_hz, cycle)),
        CelestialCommand::TimeUsage => Private("usage: /time [H:MM|auto]".to_owned()),
        CelestialCommand::MoonSet(fraction) => {
            clock.set_moon_phase_fraction(fraction, tick, server_hz, cycle);
            Public(format!("moon set to {fraction:.3}"))
        }
        CelestialCommand::MoonStatus => Private(moon_status(clock, tick, server_hz, cycle)),
        CelestialCommand::MoonUsage => Private("usage: /moon [0-1]".to_owned()),
    }
}

fn time_status(clock: &CelestialClockAnchor, tick: u32, server_hz: u32, cycle: CelestialCycleSettings) -> String {
    let time = LocalTime::from_day_fraction(clock.at_tick(tick, server_hz, cycle).solar_day_fraction).format();
    let state = if clock.running { "running" } else { "held" };
    format!("time: {time} ({state})")
}

fn moon_status(clock: &CelestialClockAnchor, tick: u32, server_hz: u32, cycle: CelestialCycleSettings) -> String {
    let fraction = clock.at_tick(tick, server_hz, cycle).lunar_phase_fraction;
    format!("moon: {fraction:.3}")
}

fn alive_players(players: &PlayerMap, name: Option<&str>) -> Vec<(PlayerId, Entity)> {
    players
        .iter()
        .filter(|(_, info)| info.connection.logged_in)
        .filter(|(_, info)| name.is_none_or(|name| info.connection.name.to_lowercase() == name.to_lowercase()))
        .filter_map(|(id, info)| info.entity().map(|entity| (*id, entity)))
        .collect()
}

fn active_players(players: &PlayerMap, name: Option<&str>) -> Vec<PlayerId> {
    players
        .iter()
        .filter(|(_, info)| info.connection.logged_in)
        .filter(|(_, info)| name.is_none_or(|name| info.connection.name.to_lowercase() == name.to_lowercase()))
        .map(|(id, _)| *id)
        .collect()
}

fn quest_status(players: &PlayerMap, board: &QuestBoard, catalog: &QuestCatalog, sender: PlayerId) -> String {
    let statuses = board.group_statuses(catalog, players);
    let own_states = players.get(&sender).map(|info| &info.session.quest_states);
    catalog
        .iter()
        .map(|quest| {
            let scope = match quest.scope {
                QuestScope::Individual => "individual",
                QuestScope::Shared => "shared",
                QuestScope::Everyone => "everyone",
            };
            let own = own_states.and_then(|states| states.get(&quest.id)).map_or_else(
                || "not assigned".to_owned(),
                |state| {
                    state.own_progress().map_or_else(
                        || "assigned".to_owned(),
                        |progress| format!("you {progress}/{}", quest.threshold),
                    )
                },
            );
            let group = statuses
                .iter()
                .find(|status| status.id == quest.id)
                .map(|status| &status.progress);
            let state = if !board.is_unlocked(&quest.id) {
                "locked".to_owned()
            } else if board.is_completed(&quest.id) {
                "completed".to_owned()
            } else {
                match group {
                    Some(QuestGroupProgress::Shared { progress }) => format!("{progress}/{}", quest.threshold),
                    Some(QuestGroupProgress::Everyone {
                        players_done,
                        players_total,
                    }) => format!("{players_done}/{players_total} players done, {own}"),
                    None => own,
                }
            };
            format!("{} ({scope}): {state}", quest.id.0)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn actor_kind_error(kind: Option<&str>, config: &ServerGameplayConfig) -> Option<String> {
    let kind = kind?;
    if config.actors.kinds.contains_key(kind) {
        return None;
    }
    let mut kinds: Vec<&str> = config.actors.kinds.keys().map(String::as_str).collect();
    kinds.sort_unstable();
    Some(format!("unknown actor kind {kind:?} (kinds: {})", kinds.join(", ")))
}

fn kill_targets(
    commands: &mut Commands,
    players: &mut PlayerMap,
    admin: &mut AdminContext,
    player_data: &PlayerStateQuery,
    targets: &[(PlayerId, Entity)],
) -> usize {
    let mut count = 0usize;
    for (id, entity) in targets {
        let Ok((pos, _, _)) = player_data.get(*entity) else {
            continue;
        };
        kill_player(
            commands,
            players,
            *id,
            *entity,
            *pos,
            admin.server_gameplay_config.player.respawn_secs,
            DeathSource::Admin,
            &admin.server_gameplay_config,
            &mut admin.pending_explosions,
        );
        count += 1;
    }
    count
}

#[cfg(test)]
#[path = "tests/execute.rs"]
mod tests;
