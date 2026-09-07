use common::{
    constants::{LADDER_CLIMB_MIN_SPEED, LADDER_WIDTH, TICK_SECS},
    protocol::{ActorMoveIntent, Position},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum WaypointKind {
    Walk,
    Mount,
    Climb {
        normal_x: f32,
        normal_z: f32,
        ascending: bool,
    },
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct NavWaypoint {
    pub position: Position,
    pub kind: WaypointKind,
}

impl NavWaypoint {
    pub const fn walk(position: Position) -> Self {
        Self {
            position,
            kind: WaypointKind::Walk,
        }
    }

    pub fn reached(self, pos: &Position, distance: f32) -> bool {
        match self.kind {
            WaypointKind::Walk => pos.horizontal_distance_sq(&self.position) <= distance * distance,
            WaypointKind::Mount => pos.horizontal_distance_sq(&self.position) <= 0.12 * 0.12,
            WaypointKind::Climb {
                normal_x,
                normal_z,
                ascending,
            } => {
                let across = (pos.x - self.position.x) * -normal_z + (pos.z - self.position.z) * normal_x;
                across.abs() <= LADDER_WIDTH / 2.0
                    && if ascending {
                        pos.y >= self.position.y
                    } else {
                        pos.y <= self.position.y
                    }
            }
            WaypointKind::Exit => {
                pos.horizontal_distance_sq(&self.position) <= distance * distance
                    && (pos.y - self.position.y).abs() <= distance
            }
        }
    }

    pub const fn is_walk(self) -> bool {
        matches!(self.kind, WaypointKind::Walk)
    }

    pub fn movement_intent(self, pos: &Position, speed: f32) -> ActorMoveIntent {
        let target = self.position;
        let direction = (target.x - pos.x).atan2(target.z - pos.z);
        let walk_speed = speed.min(pos.horizontal_distance_sq(&target).sqrt() / TICK_SECS);
        match self.kind {
            WaypointKind::Walk => ActorMoveIntent::Moving {
                direction,
                speed: walk_speed,
            },
            WaypointKind::Exit => ActorMoveIntent::ExitingLadder {
                direction,
                speed: walk_speed,
            },
            WaypointKind::Mount => ActorMoveIntent::Climbing {
                direction,
                speed: walk_speed,
            },
            WaypointKind::Climb {
                normal_x,
                normal_z,
                ascending,
            } => {
                let sign = if ascending { -1.0 } else { 1.0 };
                ActorMoveIntent::Climbing {
                    direction: (normal_x * sign).atan2(normal_z * sign),
                    speed: if self.reached(pos, 0.0) {
                        0.0
                    } else {
                        speed.max(LADDER_CLIMB_MIN_SPEED)
                    },
                }
            }
        }
    }
}
