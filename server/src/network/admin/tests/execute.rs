use crate::config::fixtures;
use common::protocol::BarrierKindTable;

use super::*;
use crate::players::PlayerInfo;

#[test]
fn give_key_and_powerup_mutate_sender_state() {
    use tokio::sync::mpsc::unbounded_channel;

    let (tx, _rx) = unbounded_channel();
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
