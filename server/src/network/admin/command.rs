use common::celestial::{LocalTime, MoonPhase};
use common::constants::CONSOLE_COMMAND_MAX_CHARS;

pub(super) const HELP_TEXT: &str = "/help\n/weather [rain|clear|auto]\n/time [HH:MM|auto]\n/moon [new|waxing_crescent|first_quarter|waxing_gibbous|full|waning_gibbous|third_quarter|waning_crescent]\n/god [on|off]\n/peace [on|off]\n/kill <name>|@a\n/killall [kind]\n/respawn [kind]\n/heal [name|@a]\n/give keys|key <color>\n/give powerups|powerup <type>\n/give missiles\n/firework\n/quest\n/quest <id> [name|@a]\n/kick <name>";

#[derive(Debug, Clone, PartialEq)]
pub(super) enum AdminCommand {
    Help,
    WeatherRain,
    WeatherClear,
    WeatherAuto,
    WeatherStatus,
    TimeSeek(LocalTime),
    TimeAuto,
    TimeStatus,
    TimeUsage,
    MoonSet(MoonPhase),
    MoonStatus,
    MoonUsage,
    God(Option<bool>),
    Peace(Option<bool>),
    KillAllPlayers,
    KillPlayer(String),
    KillActors(Option<String>),
    RespawnActors(Option<String>),
    Heal(PlayerTarget),
    GiveKeys,
    GiveKey(String),
    GivePowerups,
    GivePowerup(String),
    GiveMissiles,
    Firework,
    QuestStatus,
    CompleteQuest(String, PlayerTarget),
    Kick(String),
    MissingTarget(&'static str),
    NotACommand,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PlayerTarget {
    Sender,
    All,
    Named(String),
}

pub(super) fn parse_admin_command(input: &str) -> AdminCommand {
    let input: String = input.chars().take(CONSOLE_COMMAND_MAX_CHARS).collect();
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
        ["time"] => AdminCommand::TimeStatus,
        ["time", "auto"] => AdminCommand::TimeAuto,
        ["time", value] => LocalTime::parse(value).map_or(AdminCommand::TimeUsage, AdminCommand::TimeSeek),
        ["time", ..] => AdminCommand::TimeUsage,
        ["moon"] => AdminCommand::MoonStatus,
        ["moon", value] => MoonPhase::from_name(value).map_or(AdminCommand::MoonUsage, AdminCommand::MoonSet),
        ["moon", ..] => AdminCommand::MoonUsage,
        ["god"] => AdminCommand::God(None),
        ["god", "on"] => AdminCommand::God(Some(true)),
        ["god", "off"] => AdminCommand::God(Some(false)),
        ["peace"] => AdminCommand::Peace(None),
        ["peace", "on"] => AdminCommand::Peace(Some(true)),
        ["peace", "off"] => AdminCommand::Peace(Some(false)),
        ["kill"] => AdminCommand::MissingTarget("kill"),
        ["kill", "@a"] => AdminCommand::KillAllPlayers,
        ["kill", name @ ..] => AdminCommand::KillPlayer(name.join(" ")),
        ["killall"] => AdminCommand::KillActors(None),
        ["killall", kind] => AdminCommand::KillActors(Some((*kind).to_owned())),
        ["respawn"] => AdminCommand::RespawnActors(None),
        ["respawn", kind] => AdminCommand::RespawnActors(Some((*kind).to_owned())),
        ["heal"] => AdminCommand::Heal(PlayerTarget::Sender),
        ["heal", "@a"] => AdminCommand::Heal(PlayerTarget::All),
        ["heal", name @ ..] => AdminCommand::Heal(PlayerTarget::Named(name.join(" "))),
        ["give", "keys"] => AdminCommand::GiveKeys,
        ["give", "key", color] => AdminCommand::GiveKey((*color).to_owned()),
        ["give", "powerups"] => AdminCommand::GivePowerups,
        ["give", "powerup", power_up] => AdminCommand::GivePowerup((*power_up).to_owned()),
        ["give", "missiles"] => AdminCommand::GiveMissiles,
        ["firework"] => AdminCommand::Firework,
        ["quest"] => AdminCommand::QuestStatus,
        ["quest", id] => AdminCommand::CompleteQuest((*id).to_owned(), PlayerTarget::Sender),
        ["quest", id, "@a"] => AdminCommand::CompleteQuest((*id).to_owned(), PlayerTarget::All),
        ["quest", id, name @ ..] => AdminCommand::CompleteQuest((*id).to_owned(), PlayerTarget::Named(name.join(" "))),
        ["kick"] => AdminCommand::MissingTarget("kick"),
        ["kick", name @ ..] => AdminCommand::Kick(name.join(" ")),
        _ => AdminCommand::Unknown,
    }
}

#[cfg(test)]
#[path = "tests/command.rs"]
mod tests;
