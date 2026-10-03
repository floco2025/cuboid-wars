use std::f32::consts::PI;

use bevy::prelude::*;
use client::{
    players::{
        JumpRequest, LocalMovementReports, LocalMovementStep, OwnerBody, OwnerWorld, PlayerMotionBundle, owner_tick,
    },
    portals::portal_view_transition,
};
use common::{
    config::{GameplayConfig, NetworkConfig},
    map::Carriers,
    math::direction_from_yaw_pitch,
    physics::{CharacterMovePlan, CharacterSupport, CollisionWorld, PortalSet},
    protocol::*,
};
use serde_json::{Value, json};
use server::{actors::ActorMap, players::PlayerMap};

use super::report::point;

pub(super) struct Owner {
    pub position: Position,
    pub motion: PlayerMotionBundle,
    pub generation: PlayerGeneration,
    pub crossed_last_step: bool,
    previous_position: Position,
    jump: JumpRequest,
    step: LocalMovementStep,
    reports: LocalMovementReports,
}

impl Owner {
    pub fn new(player: &Player) -> Self {
        let mut owner = Self {
            position: player.movement.pos,
            motion: PlayerMotionBundle::from(&player.movement),
            generation: player.generation,
            crossed_last_step: false,
            previous_position: player.movement.pos,
            jump: JumpRequest::default(),
            step: LocalMovementStep::default(),
            reports: LocalMovementReports::default(),
        };
        owner.relocate(player);
        owner
    }

    pub fn relocate(&mut self, player: &Player) {
        self.position = player.movement.pos;
        self.previous_position = player.movement.pos;
        self.motion = PlayerMotionBundle::from(&player.movement);
        self.generation = player.generation;
        self.crossed_last_step = false;
        self.reports.begin_body(player.generation);
    }

    // The same body set down at rest elsewhere; the next report carries it to
    // the server, which adopts it like any other.
    pub fn teleport(&mut self, position: Position) {
        let state = PlayerMovementState::new(position, PlayerMoveIntent::NONE, 0.0, self.motion.face_yaw.0);
        self.motion = PlayerMotionBundle::from(&state);
        self.position = position;
        self.previous_position = position;
        self.crossed_last_step = false;
        self.jump = JumpRequest::default();
        self.step = LocalMovementStep::default();
    }

    // Server state supplies observed geometry, bodies and abilities. Only this
    // owner integrates the player, through the same tick as the rendered
    // client; CMove and CMoveOutcome carry its result back.
    pub fn step(
        &mut self,
        world: &World,
        id: PlayerId,
        portals: &PortalSet,
        aim: &mut Vec3,
        jump: bool,
    ) -> (Vec<ClientMessage>, Vec<Value>) {
        let player = world
            .resource::<PlayerMap>()
            .get(&id)
            .expect("experiment player missing from PlayerMap");
        let Some(entity) = player.entity() else {
            return (Vec::new(), Vec::new());
        };
        self.crossed_last_step = false;
        let gameplay = world.resource::<GameplayConfig>();
        let network = world.resource::<NetworkConfig>();
        let blockers: Vec<_> = world
            .resource::<ActorMap>()
            .values()
            .map(|actor| {
                CharacterMovePlan::stationary(
                    actor.entity,
                    *world
                        .get::<Position>(actor.entity)
                        .expect("actor position missing from its entity"),
                    0.0,
                    gameplay.expect_actor(&actor.spawn_kind).physics(),
                )
            })
            .collect();
        self.motion.move_intent.yaw = aim.x.atan2(aim.z);
        self.motion.move_intent.pitch = aim.y.clamp(-1.0, 1.0).asin();
        self.jump.pressed |= jump;
        let before = self.velocity();
        let outcome = owner_tick(
            entity,
            OwnerBody {
                position: &mut self.position,
                previous_position: &mut self.previous_position,
                intent: &mut self.motion.move_intent,
                face_yaw: &mut self.motion.face_yaw,
                vertical_velocity: &mut self.motion.vertical_velocity,
                horizontal_velocity: &mut self.motion.horizontal_velocity,
                knockback: &mut self.motion.knockback,
                stance: &mut self.motion.stance,
                support: &mut self.motion.support,
                jump: &mut self.jump,
                step: &mut self.step,
                reports: &mut self.reports,
            },
            &OwnerWorld {
                collision_world: world.resource::<CollisionWorld>(),
                carriers: world.resource::<Carriers>(),
                map_settings: world.resource::<MapSettings>(),
                gameplay_config: gameplay,
                portal_set: portals,
                network,
                open_fields: &world.resource::<SwitchState>().open_fields,
                held_keys: &player.life.held_keys,
                has_speed: player.has(PowerUpKind::Speed),
                has_low_gravity: player.has(PowerUpKind::LowGravity),
                stunned: player.is_stunned(),
                delta: network.tick_duration().as_secs_f32(),
            },
            &blockers,
        );
        let mut events = Vec::new();
        // A press is reported when made, and again when the buffer fires it later.
        if jump || outcome.jump.is_some() {
            events.push(json!({"kind": "jump", "accepted": outcome.jump.is_some()}));
        }
        if let Some(hop) = &outcome.hop {
            let (_, yaw, pitch) = portal_view_transition(
                &hop.entry,
                &hop.exit,
                aim.x.atan2(aim.z) - PI,
                aim.y.clamp(-1.0, 1.0).asin(),
                hop.yaw,
            );
            *aim = direction_from_yaw_pitch(yaw + PI, pitch);
            self.crossed_last_step = true;
            events.push(
                json!({"kind": "player_portal_crossing", "entry": hop.entry.center.to_array(),
                "exit": hop.exit.center.to_array(), "position": point(self.position),
                "velocity_before": before.to_array(), "velocity_after": self.velocity().to_array()}),
            );
        }
        let mut messages = Vec::new();
        for event in outcome.outcomes {
            events.push(match event {
                MoveOutcome::Landed { pos, impact_speed } => {
                    json!({"kind": "player_landed", "position": point(pos), "impact_speed": impact_speed})
                }
                MoveOutcome::Crushed { pos } => json!({"kind": "player_crushed", "position": point(pos)}),
                MoveOutcome::FellOutOfWorld => json!({"kind": "player_fell_out_of_world"}),
                MoveOutcome::EraseEquipment => json!({"kind": "equipment_erasure_reported"}),
            });
            messages.push(ClientMessage::MoveOutcome(CMoveOutcome {
                generation: self.generation,
                event,
            }));
        }
        if let Some(report) = outcome.report {
            messages.push(ClientMessage::Move(report));
        }
        let step = &self.step;
        events.push(
            json!({"kind": "player_step", "start": point(step.start), "position": point(self.position),
            "velocity": self.velocity().to_array(), "support": support(self.motion.support),
            "blocked": step.result.blocked, "blocked_by_actor": step.hits_character}),
        );
        (messages, events)
    }

    pub fn state(&self) -> PlayerMovementState {
        self.motion.movement_state(self.position)
    }

    fn velocity(&self) -> Vec3 {
        Vec3::Y * self.motion.vertical_velocity.0 + self.motion.horizontal_velocity.0 + self.motion.knockback.0
    }
}

pub(super) fn support(support: CharacterSupport) -> &'static str {
    match support {
        CharacterSupport::Ground => "ground",
        CharacterSupport::Airborne => "airborne",
        CharacterSupport::Ladder => "ladder",
    }
}
