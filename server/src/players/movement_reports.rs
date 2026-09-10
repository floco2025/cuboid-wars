use common::protocol::{CMove, PlayerId, sequence_is_newer};

use super::PlayerMap;

pub(crate) fn queue_player_movement(id: PlayerId, report: CMove, players: &mut PlayerMap) {
    let Some(info) = players.get_mut(&id) else {
        return;
    };
    if info.is_dead()
        || report.generation != info.session.generation
        || !report.movement.is_finite()
        || !sequence_is_newer(report.seq, info.session.last_move_seq)
    {
        return;
    }
    info.session.last_move_seq = report.seq;
    info.life.movement_report = Some(report);
}
