use bevy::prelude::*;
use common::{map::Carriers, protocol::*};
use serde_json::{Value, json};
use server::{
    actors::{ActorMap, PendingActorSpawns},
    players::PlayerMap,
    portals::PortalMap,
};

use super::session::Session;

pub(super) fn point(position: Position) -> [f32; 3] {
    [position.x, position.y, position.z]
}

pub(super) fn target(target: HitTarget) -> Value {
    match target {
        HitTarget::Actor(id) => json!({"actor": id.0}),
        HitTarget::Player { id, generation } => json!({"player": id.0, "generation": generation.0}),
    }
}

pub(super) fn portal(portal: Portal, carriers: &Carriers) -> Value {
    let pose = carriers.pose(portal.carrier);
    json!({
        "end": match portal.end { PortalEnd::A => "a", PortalEnd::B => "b" },
        "position": point(pose.transform_position(&portal.pos)),
        "normal": pose.transform_vector(Vec3::new(portal.nx, portal.ny, portal.nz)).to_array(),
        "yaw": portal.yaw, "carrier": portal.carrier.0,
    })
}

impl Session {
    pub fn record(&mut self, mut event: Value) {
        event["tick"] = json!(self.tick());
        self.events.push(event);
    }

    pub fn state(&self) -> Value {
        let world = self.server.world();
        let players = world.resource::<PlayerMap>();
        let info = players.get(&self.id).expect("experiment player missing");
        let player = info.entity().map(|entity| {
            json!({
                "position": point(self.owner.position),
                "reported_position": point(*world.get::<Position>(entity).expect("player position")),
                "yaw": self.owner.motion.face_yaw.0,
                "vertical_velocity": self.owner.motion.vertical_velocity.0,
                "momentum": self.owner.motion.airborne_momentum.0.to_array(),
                "knockback": self.owner.motion.knockback.0.to_array(),
                "support": super::player::support(self.owner.motion.support),
                "crossed_portal": self.owner.crossed_last_step,
                "checkpoint": info.session.checkpoint.number,
                "health": world.get::<Health>(entity).expect("player health").0,
                "generation": info.session.generation.0,
                "single_shot": info.has(PowerUpKind::SingleShot),
                "portal_gun": info.has(PowerUpKind::PortalGun),
            })
        });
        let mut actors: Vec<_> = world
            .resource::<ActorMap>()
            .iter()
            .map(|(id, actor)| {
                (
                    *id,
                    json!({"id": id.0, "kind": actor.spawn_kind,
                        "position": point(*world.get::<Position>(actor.entity).expect("actor position")),
                        "yaw": world.get::<FaceYaw>(actor.entity).expect("actor yaw").0,
                        "health": world.get::<Health>(actor.entity).expect("actor health").0,
                    }),
                )
            })
            .collect();
        actors.sort_by_key(|(id, _)| id.0);
        let mut spawning: Vec<_> = world.resource::<PendingActorSpawns>().0.iter().collect();
        spawning.sort_by_key(|spawn| spawn.actor_id.0);
        let spawning: Vec<_> = spawning
            .into_iter()
            .map(|spawn| {
                json!({
                    "id": spawn.actor_id.0, "kind": spawn.kind,
                    "position": point(spawn.world_position(world.resource::<Carriers>())),
                    "yaw": spawn.face_yaw, "due_tick": spawn.due_tick,
                })
            })
            .collect();
        let portals: Vec<_> = world
            .resource::<PortalMap>()
            .snapshot_portals()
            .into_iter()
            .map(|value| portal(value, world.resource::<Carriers>()))
            .collect();
        let projectiles: Vec<_> = self
            .projectiles
            .iter()
            .map(|shot| {
                json!({
                    "id": shot.id, "position": point(shot.position), "velocity": shot.motion.velocity.to_array(),
                })
            })
            .collect();
        let switch_state = world.resource::<SwitchState>();
        let active_switches: Vec<_> = switch_state
            .active_switches
            .iter()
            .map(|id| world.resource::<SwitchTable>().id(*id).expect("switch name"))
            .collect();
        let open_fields: Vec<_> = switch_state
            .open_fields
            .iter()
            .map(|id| world.resource::<FieldTable>().id(*id).expect("field name"))
            .collect();
        json!({"tick": self.tick(), "player": player, "aim": self.direction.to_array(),
            "actors": actors.into_iter().map(|(_, value)| value).collect::<Vec<_>>(),
            "spawning_actors": spawning,
            "portals": portals, "projectiles": projectiles,
            "active_switches": active_switches, "open_fields": open_fields,
        })
    }
}
