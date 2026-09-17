use common::celestial::LocalTime;
use common::constants::CONSOLE_COMMAND_MAX_CHARS;

pub(super) const HELP_TEXT: &str = "/help\n/weather [rain|clear|auto]\n/time [H:MM|auto]\n/moon [0-1]\n/god [on|off]\n/peace [on|off]\n/kill <name>|@a\n/killall [kind]\n/respawn [kind]\n/heal [name|@a]\n/checkpoint [number]\n/give keys|key <color>\n/give powerups|powerup <type>\n/give missiles\n/firework\n/quest\n/quest <id> [name|@a]\n/kick <name>";

#[derive(Debug, Clone, PartialEq)]
pub(super) enum AdminCommand {
    Help,
    WeatherRain,
    WeatherClear,
    WeatherAuto,
    WeatherStatus,
    Celestial(CelestialCommand),
    God(Option<bool>),
    Peace(Option<bool>),
    KillAllPlayers,
    KillPlayer(String),
    KillActors(Option<String>),
    RespawnActors(Option<String>),
    Heal(PlayerTarget),
    CheckpointStatus,
    SetCheckpoint(u32),
    CheckpointUsage,
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

// The clock commands, grouped so the one function that runs them is total
// over them: a new form here is a compile error there, never a missed arm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum CelestialCommand {
    TimeSeek(LocalTime),
    TimeAuto,
    TimeStatus,
    TimeUsage,
    MoonSet(f32),
    MoonStatus,
    MoonUsage,
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
        ["time"] => AdminCommand::Celestial(CelestialCommand::TimeStatus),
        ["time", "auto"] => AdminCommand::Celestial(CelestialCommand::TimeAuto),
        ["time", value] => AdminCommand::Celestial(
            LocalTime::parse(value).map_or(CelestialCommand::TimeUsage, CelestialCommand::TimeSeek),
        ),
        ["time", ..] => AdminCommand::Celestial(CelestialCommand::TimeUsage),
        ["moon"] => AdminCommand::Celestial(CelestialCommand::MoonStatus),
        ["moon", value] => AdminCommand::Celestial(
            parse_moon_fraction(value).map_or(CelestialCommand::MoonUsage, CelestialCommand::MoonSet),
        ),
        ["moon", ..] => AdminCommand::Celestial(CelestialCommand::MoonUsage),
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
        ["checkpoint"] => AdminCommand::CheckpointStatus,
        ["checkpoint", number] => number
            .parse()
            .map_or(AdminCommand::CheckpointUsage, AdminCommand::SetCheckpoint),
        ["checkpoint", ..] => AdminCommand::CheckpointUsage,
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

fn parse_moon_fraction(value: &str) -> Option<f32> {
    let fraction = value.parse::<f32>().ok()?;
    (fraction.is_finite() && (0.0..=1.0).contains(&fraction)).then_some(fraction)
}

#[cfg(test)]
#[path = "tests/command.rs"]
mod tests;
