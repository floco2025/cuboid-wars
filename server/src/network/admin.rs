use bevy::{ecs::system::SystemParam, prelude::*};

use super::incoming::PlayerStateQuery;
use crate::{
    actors::{ActorMap, ActorSpawnThrottles, PendingActorSpawns, expire_actor_spawn_cooldowns},
    combat::{DeathSource, PendingExplosions, kill_player},
    config::ServerGameplayConfig,
    map::{LightState, WeatherState, light_preset_from_str},
    network::{ServerToClient, announce, broadcast_firework_show, reply},
    players::{Invincibility, PlayerInfo, PlayerMap, UnlimitedMissiles},
};
use common::{
    config::GameplayConfig,
    protocol::{BarrierKindTable, CAdmin, FeedEvent, Health, ItemType, PlayerId, PowerUpKind},
};

// Anything longer is nonsense or abuse; truncated before parsing.
const MAX_COMMAND_CHARS: usize = 256;

// One command per line: the client feed renders each line as its own row.
// Must fit within `hud.message_feed.max_entries` or the top lines evict.
const HELP_TEXT: &str = "/help\n/weather [rain|clear|auto]\n/light [bright|dim|dark|auto|<0..1>|<from> <to> <0..1>]\n/god [on|off]\n/kill <name>|@a\n/killall [kind]\n/respawn [kind]\n/heal [name|@a]\n/give keys|key <color>\n/give powerups|powerup <type>\n/give missiles\n/firework\n/kick <name>";

// Authorization seam. Deliberately wide open for now — every client is an
// admin; tighten here (role on `PlayerInfo`, config allowlist, …) without
// touching dispatch or the protocol.
fn admin_authorized(_info: &PlayerInfo) -> bool {
    true
}

// Everything admin commands may mutate or inspect, bundled so the message
// dispatch system stays under Bevy's parameter limit.
#[derive(SystemParam)]
pub struct AdminContext<'w> {
    pub weather: ResMut<'w, WeatherState>,
    pub light: ResMut<'w, LightState>,
    pub pending_explosions: ResMut<'w, PendingExplosions>,
    pub invincibility: ResMut<'w, Invincibility>,
    pub unlimited_missiles: ResMut<'w, UnlimitedMissiles>,
    pub actor_spawn_throttles: ResMut<'w, ActorSpawnThrottles>,
    pub server_gameplay_config: Res<'w, ServerGameplayConfig>,
    pub barrier_kind_table: Res<'w, BarrierKindTable>,
}

// Vanilla-Minecraft-style command grammar: `/verb [argument]`, `@a` = all
// players, missing slash = not a command (the line stays chat-compatible).
#[derive(Debug, Clone, PartialEq)]
enum AdminCommand {
    Help,
    WeatherRain,
    WeatherClear,
    WeatherAuto,
    WeatherStatus,
    LightPreset(&'static str),
    LightFraction(f32),
    LightBlend(&'static str, &'static str, f32),
    LightAuto,
    LightStatus,
    LightUsage,
    God(Option<bool>),
    KillAllPlayers,
    KillPlayer(String),
    KillActors(Option<String>),
    RespawnActors(Option<String>),
    Heal(HealTarget),
    GiveKeys,
    GiveKey(String),
    GivePowerups,
    GivePowerup(String),
    GiveMissiles,
    Firework,
    Kick(String),
    MissingTarget(&'static str),
    NotACommand,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HealTarget {
    Sender,
    All,
    Named(String),
}

fn parse_unit_fraction(value: &str) -> Option<f32> {
    value
        .parse::<f32>()
        .ok()
        .filter(|fraction| fraction.is_finite() && (0.0..=1.0).contains(fraction))
}

fn parse_admin_command(input: &str) -> AdminCommand {
    let input: String = input.chars().take(MAX_COMMAND_CHARS).collect();
    let input = input.trim();
    let Some(command) = input.strip_prefix('/') else {
        return AdminCommand::NotACommand;
    };
    let words: Vec<&str> = command.split_whitespace().collect();

    match words.as_slice() {
        [] | ["help"] => AdminCommand::Help,
        ["weather"] => AdminCommand::WeatherStatus,
        ["weather", "rain"] => AdminCommand::WeatherRain,
        ["weather", "clear"] => AdminCommand::WeatherClear,
        ["weather", "auto"] => AdminCommand::WeatherAuto,
        ["light"] => AdminCommand::LightStatus,
        ["light", "auto"] => AdminCommand::LightAuto,
        ["light", value] => match (light_preset_from_str(value), parse_unit_fraction(value)) {
            (Some(preset), _) => AdminCommand::LightPreset(preset),
            (None, Some(fraction)) => AdminCommand::LightFraction(fraction),
            (None, None) => AdminCommand::LightUsage,
        },
        ["light", from, to, value] => {
            match (
                light_preset_from_str(from),
                light_preset_from_str(to),
                parse_unit_fraction(value),
            ) {
                (Some(from), Some(to), Some(blend)) => AdminCommand::LightBlend(from, to, blend),
                _ => AdminCommand::LightUsage,
            }
        }
        ["light", ..] => AdminCommand::LightUsage,
        ["god"] => AdminCommand::God(None),
        ["god", "on"] => AdminCommand::God(Some(true)),
        ["god", "off"] => AdminCommand::God(Some(false)),
        ["kill"] => AdminCommand::MissingTarget("kill"),
        ["kill", "@a"] => AdminCommand::KillAllPlayers,
        ["kill", name @ ..] => AdminCommand::KillPlayer(name.join(" ")),
        ["killall"] => AdminCommand::KillActors(None),
        ["killall", kind] => AdminCommand::KillActors(Some((*kind).to_owned())),
        ["respawn"] => AdminCommand::RespawnActors(None),
        ["respawn", kind] => AdminCommand::RespawnActors(Some((*kind).to_owned())),
        ["heal"] => AdminCommand::Heal(HealTarget::Sender),
        ["heal", "@a"] => AdminCommand::Heal(HealTarget::All),
        ["heal", name @ ..] => AdminCommand::Heal(HealTarget::Named(name.join(" "))),
        ["give", "keys"] => AdminCommand::GiveKeys,
        ["give", "key", color] => AdminCommand::GiveKey((*color).to_owned()),
        ["give", "powerups"] => AdminCommand::GivePowerups,
        ["give", "powerup", power_up] => AdminCommand::GivePowerup((*power_up).to_owned()),
        ["give", "missiles"] => AdminCommand::GiveMissiles,
        ["firework"] => AdminCommand::Firework,
        ["kick"] => AdminCommand::MissingTarget("kick"),
        ["kick", name @ ..] => AdminCommand::Kick(name.join(" ")),
        _ => AdminCommand::Unknown,
    }
}

// Execute one admin command. The outcome text goes back to the issuer, or
// to everyone when the command changed the shared world.
pub fn handle_admin_message(
    commands: &mut Commands,
    players: &mut PlayerMap,
    actors: &ActorMap,
    id: PlayerId,
    admin: &mut AdminContext,
    player_data: &PlayerStateQuery,
    gameplay_config: &GameplayConfig,
    map_config: &crate::map::MapConfig,
    pending_actor_spawns: &mut PendingActorSpawns,
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
            player_data,
            gameplay_config,
            map_config,
            pending_actor_spawns,
            &msg.command,
        )
    } else {
        AdminOutcome::Private("not authorized".to_owned())
    };
    let feed = &admin.server_gameplay_config.feed;
    match outcome {
        AdminOutcome::Public(text) if feed.admin_action => announce(
            players,
            feed,
            FeedEvent::AdminAction {
                name: players.display_name(&id),
                text,
            },
        ),
        // With announcements switched off the issuer still gets the outcome.
        AdminOutcome::Public(text) | AdminOutcome::Private(text) => {
            if let Some(info) = players.get(&id) {
                reply(info, FeedEvent::AdminReply { text });
            }
        }
    }
}

// Where a command's outcome text goes: back to the issuer, or to everyone
// (world-affecting commands, on success).
enum AdminOutcome {
    Private(String),
    Public(String),
}

fn run_admin_command(
    commands: &mut Commands,
    players: &mut PlayerMap,
    actors: &ActorMap,
    sender: PlayerId,
    admin: &mut AdminContext,
    player_data: &PlayerStateQuery,
    gameplay_config: &GameplayConfig,
    map_config: &crate::map::MapConfig,
    pending_actor_spawns: &mut PendingActorSpawns,
    command: &str,
) -> AdminOutcome {
    use AdminOutcome::{Private, Public};

    match parse_admin_command(command) {
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
        AdminCommand::LightPreset(name) => {
            admin.light.hold_preset(name);
            Public(format!("light set to {name}"))
        }
        AdminCommand::LightFraction(fraction) => {
            admin.light.hold_cycle_fraction(fraction);
            Public(admin.light.status())
        }
        AdminCommand::LightBlend(from, to, blend) => {
            admin.light.hold_blend(from, to, blend);
            Public(admin.light.status())
        }
        AdminCommand::LightAuto => match admin.light.resume_auto() {
            Ok(()) => Public("light cycle resumed".to_owned()),
            Err(reason) => Private(reason.to_owned()),
        },
        AdminCommand::LightStatus => Private(admin.light.status()),
        AdminCommand::LightUsage => {
            Private("usage: /light [bright|dim|dark|auto|<0..1>|<from> <to> <0..1>]".to_owned())
        }
        AdminCommand::God(explicit) => {
            let enabled = explicit.unwrap_or(!admin.invincibility.0);
            admin.invincibility.0 = enabled;
            admin.unlimited_missiles.0 = enabled;
            Public(format!("god mode {}", if enabled { "on" } else { "off" }))
        }
        AdminCommand::KillAllPlayers => {
            let targets = alive_players(players, None);
            let count = kill_targets(commands, players, admin, player_data, gameplay_config, &targets);
            Public(format!("killed {count} player(s)"))
        }
        AdminCommand::KillPlayer(name) => {
            let targets = alive_players(players, Some(&name));
            if targets.is_empty() {
                return Private(format!("unknown player {name:?}"));
            }
            let count = kill_targets(commands, players, admin, player_data, gameplay_config, &targets);
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
            let count = expire_actor_spawn_cooldowns(
                actors,
                pending_actor_spawns,
                &mut admin.actor_spawn_throttles,
                map_config,
                &admin.server_gameplay_config,
                kind.as_deref(),
            );
            Public(format!("respawning {count} actor(s)"))
        }
        AdminCommand::Heal(target) => {
            let targets = match &target {
                HealTarget::Sender => alive_players(players, None)
                    .into_iter()
                    .filter(|(id, _)| *id == sender)
                    .collect(),
                HealTarget::All => alive_players(players, None),
                HealTarget::Named(name) => {
                    let targets = alive_players(players, Some(name));
                    if targets.is_empty() {
                        return Private(format!("unknown player {name:?}"));
                    }
                    targets
                }
            };
            let max_health = gameplay_config.player.health().max;
            for (_, entity) in &targets {
                commands.entity(*entity).insert(Health(max_health));
            }
            let text = format!("healed {} player(s)", targets.len());
            // Healing only yourself is nobody else's business.
            if targets.iter().any(|(id, _)| *id != sender) {
                Public(text)
            } else {
                Private(text)
            }
        }
        AdminCommand::GiveKeys => {
            let Some(info) = players.get_mut(&sender) else {
                return Private("sender not found".to_owned());
            };
            let mut added = 0usize;
            for index in 0..admin.barrier_kind_table.len() {
                if let Ok(kind) = u16::try_from(index)
                    && info.add_key(common::protocol::BarrierKindId(kind))
                {
                    added += 1;
                }
            }
            Private(format!("gave {added} key(s)"))
        }
        AdminCommand::GiveKey(color) => match admin.barrier_kind_table.index_of(&color) {
            Some(kind) => {
                let Some(info) = players.get_mut(&sender) else {
                    return Private("sender not found".to_owned());
                };
                Private(if info.add_key(kind) {
                    format!("gave the {color} key")
                } else {
                    format!("already holding the {color} key")
                })
            }
            None => Private(format!(
                "unknown key color {color:?} (colors: {})",
                admin.barrier_kind_table.ids().join(", ")
            )),
        },
        AdminCommand::GivePowerups => {
            let Some(info) = players.get_mut(&sender) else {
                return Private("sender not found".to_owned());
            };
            let mut given = 0usize;
            for kind in PowerUpKind::ALL {
                let id = kind.to_item_type().config_id();
                grant_power_up_by_id(info, id, &admin.server_gameplay_config);
                given += 1;
            }
            Private(format!("gave {given} power-ups"))
        }
        AdminCommand::GivePowerup(power_up) => {
            let power_up_ids = PowerUpKind::ALL.map(|kind| kind.to_item_type().config_id());
            if !power_up_ids.contains(&power_up.as_str()) {
                return Private(format!(
                    "unknown power-up {power_up:?} (power-ups: {})",
                    power_up_ids.join(", ")
                ));
            }
            let Some(info) = players.get_mut(&sender) else {
                return Private("sender not found".to_owned());
            };
            grant_power_up_by_id(info, &power_up, &admin.server_gameplay_config);
            Private(format!("gave the {power_up} power-up"))
        }
        AdminCommand::GiveMissiles => {
            let Some(info) = players.get_mut(&sender) else {
                return Private("sender not found".to_owned());
            };
            let max = gameplay_config.missiles.max_missiles;
            // No `SMissilesCollected` cue — that would play the pickup sound
            // on the client (admin gives are silent, like keys/power-ups);
            // the next snapshot updates the HUD.
            let missiles = info.add_missiles(max, max);
            Private(format!("gave missiles ({missiles}/{max})"))
        }
        AdminCommand::Firework => {
            broadcast_firework_show(players);
            Public("launched fireworks".to_owned())
        }
        AdminCommand::Kick(name) => {
            let mut count = 0usize;
            for (_, info) in players.iter() {
                if info.logged_in && info.name.to_lowercase() == name.to_lowercase() {
                    let _ = info.channel.send(ServerToClient::Close);
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

// Logged-in, alive players — all of them, or only those matching `name`
// (case-insensitive; names aren't unique, so several can match).
fn alive_players(players: &PlayerMap, name: Option<&str>) -> Vec<(PlayerId, Entity)> {
    players
        .iter()
        .filter(|(_, info)| info.logged_in && !info.is_dead())
        .filter(|(_, info)| name.is_none_or(|name| info.name.to_lowercase() == name.to_lowercase()))
        .map(|(id, info)| (*id, info.entity))
        .collect()
}

fn actor_kind_error(kind: Option<&str>, config: &ServerGameplayConfig) -> Option<String> {
    let kind = kind?;
    if config.actors.contains_key(kind) {
        return None;
    }
    let mut kinds: Vec<&str> = config.actors.keys().map(String::as_str).collect();
    kinds.sort_unstable();
    Some(format!("unknown actor kind {kind:?} (kinds: {})", kinds.join(", ")))
}

fn kill_targets(
    commands: &mut Commands,
    players: &mut PlayerMap,
    admin: &mut AdminContext,
    player_data: &PlayerStateQuery,
    gameplay_config: &GameplayConfig,
    targets: &[(PlayerId, Entity)],
) -> usize {
    let mut count = 0usize;
    for (id, entity) in targets {
        let Ok((pos, _, _, _)) = player_data.get(*entity) else {
            continue;
        };
        kill_player(
            commands,
            players,
            *id,
            *entity,
            *pos,
            gameplay_config.player.respawn_delay_secs,
            DeathSource::Admin,
            &admin.server_gameplay_config.feed,
            &mut admin.pending_explosions,
        );
        count += 1;
    }
    count
}

fn grant_power_up_by_id(info: &mut PlayerInfo, id: &str, config: &ServerGameplayConfig) {
    if let Some(item_type) = ItemType::from_config_id(id) {
        info.grant_power_up(item_type, &config.power_ups);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_command_form() {
        assert_eq!(parse_admin_command("/help"), AdminCommand::Help);
        assert_eq!(parse_admin_command("  /  "), AdminCommand::Help);
        assert_eq!(parse_admin_command("/weather rain"), AdminCommand::WeatherRain);
        assert_eq!(parse_admin_command("/weather clear"), AdminCommand::WeatherClear);
        assert_eq!(parse_admin_command("/weather auto"), AdminCommand::WeatherAuto);
        assert_eq!(parse_admin_command("/weather"), AdminCommand::WeatherStatus);
        assert_eq!(
            parse_admin_command("/light bright"),
            AdminCommand::LightPreset("bright")
        );
        assert_eq!(parse_admin_command("/light dim"), AdminCommand::LightPreset("dim"));
        assert_eq!(parse_admin_command("/light dark"), AdminCommand::LightPreset("dark"));
        assert_eq!(parse_admin_command("/light auto"), AdminCommand::LightAuto);
        assert_eq!(parse_admin_command("/light"), AdminCommand::LightStatus);
        assert_eq!(parse_admin_command("/light 0.7"), AdminCommand::LightFraction(0.7));
        assert_eq!(
            parse_admin_command("/light dim dark 0.3"),
            AdminCommand::LightBlend("dim", "dark", 0.3)
        );
        assert_eq!(parse_admin_command("/light 1.5"), AdminCommand::LightUsage);
        assert_eq!(parse_admin_command("/light banana"), AdminCommand::LightUsage);
        assert_eq!(parse_admin_command("/light dim banana 0.3"), AdminCommand::LightUsage);
        assert_eq!(parse_admin_command("/light dim dark"), AdminCommand::LightUsage);
        assert_eq!(parse_admin_command("/god"), AdminCommand::God(None));
        assert_eq!(parse_admin_command("/god on"), AdminCommand::God(Some(true)));
        assert_eq!(parse_admin_command("/god off"), AdminCommand::God(Some(false)));
        assert_eq!(parse_admin_command("/kill @a"), AdminCommand::KillAllPlayers);
        assert_eq!(
            parse_admin_command("/kill Bob the Great"),
            AdminCommand::KillPlayer("Bob the Great".to_owned())
        );
        assert_eq!(parse_admin_command("/kill"), AdminCommand::MissingTarget("kill"));
        assert_eq!(parse_admin_command("/killall"), AdminCommand::KillActors(None));
        assert_eq!(
            parse_admin_command("/killall zapper"),
            AdminCommand::KillActors(Some("zapper".to_owned()))
        );
        assert_eq!(parse_admin_command("/respawn"), AdminCommand::RespawnActors(None));
        assert_eq!(
            parse_admin_command("/respawn sentry"),
            AdminCommand::RespawnActors(Some("sentry".to_owned()))
        );
        assert_eq!(parse_admin_command("/heal"), AdminCommand::Heal(HealTarget::Sender));
        assert_eq!(parse_admin_command("/heal @a"), AdminCommand::Heal(HealTarget::All));
        assert_eq!(
            parse_admin_command("/heal Bob"),
            AdminCommand::Heal(HealTarget::Named("Bob".to_owned()))
        );
        assert_eq!(parse_admin_command("/give keys"), AdminCommand::GiveKeys);
        assert_eq!(
            parse_admin_command("/give key lobby"),
            AdminCommand::GiveKey("lobby".to_owned())
        );
        assert_eq!(parse_admin_command("/give powerups"), AdminCommand::GivePowerups);
        assert_eq!(parse_admin_command("/give missiles"), AdminCommand::GiveMissiles);
        assert_eq!(
            parse_admin_command("/give powerup speed"),
            AdminCommand::GivePowerup("speed".to_owned())
        );
        assert_eq!(parse_admin_command("/kick Bob"), AdminCommand::Kick("Bob".to_owned()));
        assert_eq!(parse_admin_command("/kick"), AdminCommand::MissingTarget("kick"));
    }

    #[test]
    fn slashless_input_is_not_a_command() {
        assert_eq!(parse_admin_command("hello there"), AdminCommand::NotACommand);
        assert_eq!(parse_admin_command(""), AdminCommand::NotACommand);
        assert_eq!(parse_admin_command("kill @a"), AdminCommand::NotACommand);
    }

    #[test]
    fn unknown_and_overlong_input_parse_safely() {
        assert_eq!(parse_admin_command("/dance"), AdminCommand::Unknown);
        assert_eq!(parse_admin_command("/give"), AdminCommand::Unknown);
        assert_eq!(
            parse_admin_command(&format!("/{}", "x".repeat(10_000))),
            AdminCommand::Unknown
        );
    }

    #[test]
    fn give_key_and_powerup_mutate_sender_state() {
        use tokio::sync::mpsc::unbounded_channel;

        let (tx, _rx) = unbounded_channel();
        let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
        let table = BarrierKindTable::from_ids(vec!["lobby".to_owned(), "basement".to_owned()])
            .expect("test barrier kind table should build");

        let kind = table.index_of("lobby").expect("lobby kind missing from test table");
        assert!(info.add_key(kind));
        assert!(!info.add_key(kind), "second add of the same key must be a no-op");

        let config = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        grant_power_up_by_id(&mut info, "speed", &config);
        assert!(
            info.power_up_timers[common::protocol::PowerUpKind::Speed.index()] > 0.0,
            "speed timer must be armed"
        );
    }

    #[test]
    fn god_toggles_and_sets() {
        fn apply(current: bool, explicit: Option<bool>) -> bool {
            explicit.unwrap_or(!current)
        }
        assert!(apply(false, None), "bare god must toggle on");
        assert!(!apply(true, None), "bare god must toggle off");
        assert!(apply(false, Some(true)), "god on must set on");
        assert!(!apply(true, Some(false)), "god off must set off");
    }
}
