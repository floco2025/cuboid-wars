use common::{
    physics::ProjectileCharacterHit,
    protocol::{ActorId, PlayerId},
};

#[derive(Clone, Copy)]
pub(super) enum ProjectileTargetHit {
    Player { id: PlayerId, hit: ProjectileCharacterHit },
    Actor { id: ActorId, hit: ProjectileCharacterHit },
}

impl ProjectileTargetHit {
    const fn hit(self) -> ProjectileCharacterHit {
        match self {
            Self::Player { hit, .. } | Self::Actor { hit, .. } => hit,
        }
    }
}

pub(super) fn closer_hit(current: Option<ProjectileTargetHit>, candidate: ProjectileTargetHit) -> ProjectileTargetHit {
    match current {
        Some(current) if current.hit().time_of_impact <= candidate.hit().time_of_impact => current,
        _ => candidate,
    }
}
