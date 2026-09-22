use std::f32::consts::PI;

use bevy::prelude::*;
use client::{
    players::{
        LocalMovementReports, LocalMovementStep, PlayerMotionBundle, PlayerMovementStep, collect_move_outcomes,
        momentum_displacement, plan_player_move, player_movement_state,
    },
    portals::portal_view_transition,
};
use common::{
    config::{GameplayConfig, NetworkConfig, UpdateCadence},
    map::Carriers,
    math::direction_from_yaw_pitch,
    physics::{
        CharacterMovePlan, CharacterSupport, CollisionWorld, PlayerHopBody, PortalSet, passable_fields,
        player_control_velocity, player_jump_velocity,
    },
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
    reports: LocalMovementReports,
    cadence: UpdateCadence,
    eraser_cadence: Option<UpdateCadence>,
}

impl Owner {
    pub fn new(player: &Player, network: &NetworkConfig) -> Self {
        let mut owner = Self {
            position: player.movement.pos,
            motion: PlayerMotionBundle::from(&player.movement),
            generation: player.generation,
            crossed_last_step: false,
            reports: LocalMovementReports::default(),
            cadence: network.update_cadence(),
            eraser_cadence: None,
        };
        owner.relocate(player);
        owner
    }

    pub fn relocate(&mut self, player: &Player) {
        self.position = player.movement.pos;
        self.motion = PlayerMotionBundle::from(&player.movement);
        self.generation = player.generation;
        self.crossed_last_step = false;
        self.reports.begin_body(player.generation);
    }

    pub fn state(&self) -> PlayerMovementState {
        player_movement_state(
            self.position,
            self.motion.move_intent,
            &self.motion.face_yaw,
            &self.motion.vertical_velocity,
            &self.motion.airborne_momentum,
            &self.motion.knockback,
            self.motion.support,
        )
    }

    // Server state supplies observed geometry, bodies and abilities. Only this
    // owner integrates the player; CMove and CMoveOutcome carry its result back.
    pub fn step(
        &mut self,
        world: &World,
        id: PlayerId,
        portals: &PortalSet,
        aim: &mut Vec3,
        jump: bool,
    ) -> (Vec<ClientMessage>, Vec<Value>) {
        let player = world.resource::<PlayerMap>().get(&id).expect("experiment player");
        let Some(entity) = player.entity() else {
            return (Vec::new(), Vec::new());
        };
        self.crossed_last_step = false;
        let collision = world.resource::<CollisionWorld>();
        let carriers = world.resource::<Carriers>();
        let gameplay = world.resource::<GameplayConfig>();
        let settings = world.resource::<MapSettings>();
        let network = world.resource::<NetworkConfig>();
        let delta = network.tick_duration().as_secs_f32();
        let open = &world.resource::<SwitchState>().open_fields;
        let has_speed = player.has(PowerUpKind::Speed);
        let stunned = player.is_stunned();
        let mut events = Vec::new();
        if jump {
            let passable = passable_fields(&player.life.held_keys, open);
            let velocity = (!stunned)
                .then(|| {
                    player_jump_velocity(
                        self.motion.vertical_velocity.0,
                        collision,
                        gameplay.player.physics(),
                        settings.movement.player.jump_speed,
                        &self.position,
                        &passable,
                    )
                })
                .flatten();
            if let Some(velocity) = velocity {
                self.motion.vertical_velocity.0 = velocity;
            }
            events.push(json!({"kind": "jump", "accepted": velocity.is_some()}));
        }
        if self.motion.support == CharacterSupport::Ladder
            && let Some(ladder) = collision.ladder_volume_at(&self.position)
        {
            self.motion.face_yaw.0 = (-ladder.normal_x).atan2(-ladder.normal_z);
        }
        let control = player_control_velocity(self.motion.move_intent, &settings.movement, has_speed, stunned);
        let blockers: Vec<_> = world
            .resource::<ActorMap>()
            .values()
            .map(|actor| {
                CharacterMovePlan::stationary(
                    actor.entity,
                    *world.get::<Position>(actor.entity).expect("actor position"),
                    0.0,
                    gameplay.expect_actor(&actor.spawn_kind).physics(),
                )
            })
            .collect();
        let start = self.position;
        let planned = plan_player_move(
            entity,
            PlayerMovementStep {
                start,
                vertical_velocity: self.motion.vertical_velocity.0,
                control_velocity: control,
                delta,
                has_low_gravity: player.has(PowerUpKind::LowGravity),
                held_keys: &player.life.held_keys,
                open_fields: open,
                external_displacement: momentum_displacement(
                    Some(&self.motion.knockback),
                    Some(&self.motion.airborne_momentum),
                    delta,
                ),
                collision_world: collision,
                map_settings: settings,
                gameplay_config: gameplay,
                portal_set: portals,
                carriers,
            },
            &blockers,
        );
        let result = planned.result;
        self.position = result.position;
        self.motion.vertical_velocity.0 = result.vertical_velocity;
        self.motion.support = result.support;
        self.motion.airborne_momentum.finish_step(&result);
        if planned.hits_character {
            self.motion.airborne_momentum.0 = Vec3::ZERO;
        }

        if let Some(hop) = portals.player_hop(
            start.into(),
            self.position.into(),
            gameplay,
            &settings.movement,
            PlayerHopBody {
                move_intent: self.motion.move_intent,
                has_speed,
                stunned,
                knockback: &self.motion.knockback,
                airborne_momentum: &self.motion.airborne_momentum,
                vertical_velocity: self.motion.vertical_velocity.0,
                yaw: self.motion.face_yaw.0,
            },
        ) {
            let entrance = self.position;
            let before = self.velocity(settings, has_speed, stunned);
            hop.apply_player_state(
                &mut self.position,
                &mut self.motion.face_yaw,
                &mut self.motion.vertical_velocity,
                &mut self.motion.move_intent,
            );
            hop.apply_motion_components(&mut self.motion.knockback, &mut self.motion.airborne_momentum);
            let (_, yaw, pitch) = portal_view_transition(
                &hop.entry,
                &hop.exit,
                aim.x.atan2(aim.z) - PI,
                aim.y.clamp(-1.0, 1.0).asin(),
                hop.yaw,
            );
            *aim = direction_from_yaw_pitch(yaw + PI, pitch);
            self.reports.begin_crossing(entrance);
            self.crossed_last_step = true;
            events.push(json!({"kind": "player_portal_crossing", "entry": hop.entry.center.to_array(),
                "exit": hop.exit.center.to_array(), "position": point(self.position),
                "velocity_before": before.to_array(), "velocity_after": self.velocity(settings, has_speed, stunned).to_array()}));
        }
        let step = LocalMovementStep {
            start,
            crushed: result.crushed,
            impact_speed: result.impact_speed,
            carrier: result.carrier,
            support: result.support,
        };
        let outcomes = collect_move_outcomes(
            &self.position,
            &step,
            gameplay.player.physics(),
            collision,
            carriers,
            &mut self.reports,
            &mut self.eraser_cadence,
            network,
        );
        let mut messages = Vec::new();
        for event in outcomes {
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
        let movement = self.state();
        if let Some(report) = self
            .reports
            .movement_report(&mut self.cadence, movement, result.carrier, carriers)
        {
            messages.push(ClientMessage::Move(report));
        }
        events.push(
            json!({"kind": "player_step", "start": point(start), "position": point(self.position),
            "velocity": self.velocity(settings, has_speed, stunned).to_array(), "support": support(self.motion.support),
            "blocked": result.blocked, "blocked_by_actor": planned.hits_character}),
        );
        self.motion
            .knockback
            .decay(delta, settings.movement.knockback.deceleration);
        (messages, events)
    }

    fn velocity(&self, settings: &MapSettings, has_speed: bool, stunned: bool) -> Vec3 {
        player_control_velocity(self.motion.move_intent, &settings.movement, has_speed, stunned)
            + Vec3::Y * self.motion.vertical_velocity.0
            + self.motion.airborne_momentum.0
            + self.motion.knockback.0
    }
}

pub(super) fn support(support: CharacterSupport) -> &'static str {
    match support {
        CharacterSupport::Ground => "ground",
        CharacterSupport::Airborne => "airborne",
        CharacterSupport::Ladder => "ladder",
    }
}
