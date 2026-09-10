use common::{
    map::Carriers,
    protocol::{CMove, PlayerId, sequence_is_newer},
};

use super::PlayerMap;

pub(crate) fn queue_player_movement(id: PlayerId, report: CMove, players: &mut PlayerMap, carriers: &Carriers) {
    let Some(info) = players.get_mut(&id) else {
        return;
    };
    if info.is_dead()
        || report.generation != info.session.generation
        || !report.movement.is_finite()
        || !carriers.contains(report.movement.carrier)
        || !sequence_is_newer(report.seq, info.session.last_move_seq)
    {
        return;
    }
    info.session.last_move_seq = report.seq;
    info.life.movement = report.movement;
    info.life.portal_crossing = report.portal_crossing;
}
