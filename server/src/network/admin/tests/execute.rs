use crate::config::fixtures;
use common::protocol::BarrierKindTable;

use super::*;
use crate::players::PlayerInfo;

#[test]
fn give_key_and_powerup_mutate_sender_state() {
    use crossbeam_channel::unbounded;

    let (tx, _rx) = unbounded();
    let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
    let table = BarrierKindTable::from_ids(vec!["lobby".to_owned(), "basement".to_owned()])
        .expect("test barrier kind table failed to build");

    let kind = table.index_of("lobby").expect("lobby kind missing from test table");
    assert!(info.add_key(kind));
    assert!(!info.add_key(kind), "second add of the same key must be a no-op");

    let config = fixtures::server_config();
    info.grant_power_up(ItemType::SpeedPowerUp, &config.maps["hotel"].power_ups);
    assert!(
        info.has(common::protocol::PowerUpKind::Speed),
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

fn celestial_fixture() -> (CelestialClockAnchor, CelestialCycleSettings) {
    (
        CelestialClockAnchor {
            anchor_tick: 100,
            solar_day_fraction: 0.5,
            lunar_phase_fraction: 0.25,
            running: true,
        },
        CelestialCycleSettings {
            day_duration_secs: 600.0,
            lunar_cycle_days: 8.0,
        },
    )
}

#[test]
fn celestial_status_commands_report_extrapolated_time_phase_and_run_state() {
    let (mut clock, cycle) = celestial_fixture();
    assert_eq!(
        run_celestial_command(CelestialCommand::TimeStatus, &mut clock, 100, 30, cycle),
        AdminOutcome::Private("time: 12:00 (running)".to_owned())
    );
    assert_eq!(
        run_celestial_command(CelestialCommand::MoonStatus, &mut clock, 100, 30, cycle),
        AdminOutcome::Private("moon: 0.250".to_owned())
    );
}

#[test]
fn celestial_mutations_announce_publicly_and_preserve_held_state() {
    let (mut clock, cycle) = celestial_fixture();
    let time = LocalTime::parse("23:30").expect("valid fixture time");
    assert_eq!(
        run_celestial_command(CelestialCommand::TimeSeek(time), &mut clock, 130, 30, cycle),
        AdminOutcome::Public("time set to 23:30 (held)".to_owned())
    );
    assert!(!clock.running);

    assert_eq!(
        run_celestial_command(CelestialCommand::MoonSet(0.5), &mut clock, 160, 30, cycle),
        AdminOutcome::Public("moon set to 0.500".to_owned())
    );
    assert_eq!(clock.lunar_phase_fraction, 0.5);
    assert!(!clock.running, "rephasing a held clock must leave it held");

    assert_eq!(
        run_celestial_command(CelestialCommand::TimeAuto, &mut clock, 160, 30, cycle),
        AdminOutcome::Public("time resumed".to_owned())
    );
    assert!(clock.running);
    assert_eq!(
        run_celestial_command(CelestialCommand::TimeAuto, &mut clock, 160, 30, cycle),
        AdminOutcome::Private("time already running".to_owned())
    );
}
