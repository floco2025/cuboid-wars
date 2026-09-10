use common::protocol::{CMoveOutcome, MoveOutcome, PlayerId, Position};

use super::PlayerMap;

#[derive(Default)]
pub(crate) struct PendingOutcomes {
    pub landings: Vec<Landing>,
    pub crushed: Option<Position>,
    pub fell_out_of_world: bool,
    pub erase_equipment: bool,
}

pub(crate) struct Landing {
    pub pos: Position,
    pub impact_speed: f32,
}

pub(crate) fn handle_move_outcome(id: PlayerId, message: CMoveOutcome, players: &mut PlayerMap) {
    let Some(info) = players.get_mut(&id) else { return };
    if info.is_dead() || info.session.generation != message.generation {
        return;
    }
    let outcomes = &mut info.life.outcomes;
    match message.event {
        MoveOutcome::Landed { pos, impact_speed }
            if pos.is_finite() && impact_speed.is_finite() && impact_speed >= 0.0 =>
        {
            outcomes.landings.push(Landing { pos, impact_speed });
        }
        MoveOutcome::Crushed { pos } if pos.is_finite() => outcomes.crushed = Some(pos),
        MoveOutcome::FellOutOfWorld => outcomes.fell_out_of_world = true,
        MoveOutcome::EraseEquipment => outcomes.erase_equipment = true,
        _ => {}
    }
}

#[cfg(test)]
#[path = "tests/outcomes.rs"]
mod tests;
