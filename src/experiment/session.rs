use anyhow::{Context, Result, ensure};
use bevy::prelude::*;
use client::{
    players::CrouchBlend,
    projectiles::{
        MuzzleCheck, ProjectileEnvironment, ProjectileFlightEvent, ProjectileMotion, calculate_projectile_spawns,
        projectile_character_hit, projectile_overlaps_character, step_projectile,
    },
};
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig, NetworkConfig},
    constants::CHARACTER_KNOCKBACK_CLAMP_RATIO,
    map::Carriers,
    physics::{
        CharacterSupport, CollisionWorld, PortalPlacementFailure, PortalSet, compute_portal_placement, passable_fields,
        portal_placement_overlaps,
    },
    protocol::*,
};
use crossbeam_channel::{Receiver, Sender, unbounded};
use serde_json::{Value, json};
use server::{actors::ActorMap, network::LocalLink, players::PlayerMap, portals::PortalMap};

use super::{
    player::Owner,
    report::{point, portal, target},
    script::{End, Script},
};

pub(super) struct Shot {
    pub id: u64,
    pub position: Position,
    pub motion: ProjectileMotion,
}

pub(super) struct Session {
    pub server: App,
    pub bootstrap: SInit,
    pub visual_messages: Option<Vec<ServerMessage>>,
    pub id: PlayerId,
    pub direction: Vec3,
    pub projectiles: Vec<Shot>,
    pub events: Vec<Value>,
    pub owner: Owner,
    jump_requested: bool,
    to_server: Sender<ClientMessage>,
    from_server: Receiver<ServerMessage>,
    access: PortalAccess,
    last_shot_time: f32,
    next_shot: u64,
}

impl Session {
    pub fn new(script: &Script) -> Result<Self> {
        let (to_server, from_client) = unbounded();
        let (to_client, from_server) = unbounded();
        let mut server = script.build_server(LocalLink { to_client, from_client }, false)?;
        ensure!(
            server.world().resource::<MapLayout>().carriers.is_empty(),
            "this experiment runner does not yet support carriers"
        );
        to_server.send(ClientMessage::Login(CLogin {
            name: "Experiment".into(),
        }))?;
        server.update();
        let bootstrap = from_server
            .try_iter()
            .find_map(|message| match message {
                ServerMessage::Init(init) => Some(init),
                _ => None,
            })
            .context("server did not send experiment bootstrap")?;
        let relocated = from_server
            .try_iter()
            .find_map(|message| match message {
                ServerMessage::PlayerRelocated(value) if value.id == bootstrap.player.id => Some(value.player),
                _ => None,
            })
            .context("server did not establish the initial player body")?;
        let owner = Owner::new(&relocated);
        let id = bootstrap.player.id;
        let access = bootstrap.player.portal_access;
        let mut session = Self {
            server,
            bootstrap,
            visual_messages: None,
            owner,
            jump_requested: false,
            id,
            access,
            direction: Vec3::Z,
            projectiles: Vec::new(),
            events: Vec::new(),
            to_server,
            from_server,
            last_shot_time: f32::NEG_INFINITY,
            next_shot: 0,
        };
        session.receive()?;
        session.validate_initial_position()?;
        session.events.clear();
        Ok(session)
    }

    pub fn tick(&self) -> u32 {
        self.server.world().resource::<ServerTick>().0
    }

    pub(super) fn eye(&self) -> Result<Vec3> {
        let world = self.server.world();
        let player = world
            .resource::<PlayerMap>()
            .get(&self.id)
            .context("experiment player missing")?;
        ensure!(!player.is_dead(), "player is dead");
        Ok(Vec3::from(self.owner.position)
            + Vec3::Y
                * CrouchBlend::settled(self.owner.motion.stance).eye_height(&world.resource::<GameplayConfig>().player))
    }

    pub fn aim(&mut self, position: [f32; 3]) -> Result<Value> {
        self.direction = (Vec3::from_array(position) - self.eye()?)
            .try_normalize()
            .context("aim target must differ from the player's eye position")?;
        Ok(json!({"status": "aimed", "direction": self.direction.to_array()}))
    }

    fn ready(&self, ability: PowerUpKind) -> Option<&'static str> {
        let world = self.server.world();
        let player = world
            .resource::<PlayerMap>()
            .get(&self.id)
            .expect("experiment player missing");
        if player.is_dead() {
            return Some("player_dead");
        }
        if !player.has(ability) {
            return Some("missing_equipment");
        }
        let now = world.resource::<Time>().elapsed_secs();
        if now - self.last_shot_time < world.resource::<GameplayConfig>().projectiles.cooldown_secs {
            return Some("cooldown");
        }
        None
    }

    fn yaw(&self) -> f32 {
        yaw_of(self.direction)
    }

    pub fn portal(&mut self, end: End) -> Result<Value> {
        if let Some(reason) = self.ready(PowerUpKind::PortalGun) {
            return Ok(json!({"status": "rejected", "reason": reason}));
        }
        self.shoot_portal(end, self.eye()?, self.direction)
    }

    pub fn place(&mut self, end: End, eye: [f32; 3], target: [f32; 3]) -> Result<Value> {
        if let Some(reason) = self.ready(PowerUpKind::PortalGun) {
            return Ok(json!({"status": "rejected", "reason": reason}));
        }
        let eye = Vec3::from_array(eye);
        let direction = (Vec3::from_array(target) - eye)
            .try_normalize()
            .context("place target must differ from its eye")?;
        self.shoot_portal(end, eye, direction)
    }

    fn shoot_portal(&mut self, end: End, origin: Vec3, direction: Vec3) -> Result<Value> {
        let end = match end {
            End::A => PortalEnd::A,
            End::B => PortalEnd::B,
        };
        let Some(pair) = self.access.pair().filter(|_| self.access.allows(end)) else {
            return Ok(json!({"status": "rejected", "reason": "portal_access"}));
        };
        let world = self.server.world();
        let carriers = world.resource::<Carriers>();
        let existing = world.resource::<PortalMap>().snapshot_portals();
        let placement = compute_portal_placement(
            origin,
            direction,
            yaw_of(direction),
            &world.resource::<GameplayConfig>().portals,
            world.resource::<CollisionWorld>(),
            world.resource::<MapLayout>(),
            carriers,
            &world.resource::<SwitchState>().open_fields,
            &world.resource::<MapSettings>().textures,
        );
        let (result, response) = match placement {
            Ok(placement) => {
                let placed = placement.portal(pair, end, carriers);
                if portal_placement_overlaps(
                    &placed,
                    &existing,
                    carriers,
                    world.resource::<GameplayConfig>().portals.size,
                ) {
                    return Ok(json!({"status": "rejected", "reason": "portal_overlap"}));
                }
                (
                    PortalShotResult::Placed(placed),
                    json!({"status": "submitted", "portal": portal(placed, carriers)}),
                )
            }
            Err(PortalPlacementFailure::InvalidPlacement) => {
                return Ok(json!({"status": "rejected", "reason": "invalid_placement"}));
            }
            Err(PortalPlacementFailure::IncompatibleMaterial(impact)) => (
                PortalShotResult::Fizzled(impact.portal(pair, end, carriers)),
                json!({"status": "fizzled", "reason": "incompatible_material"}),
            ),
        };
        self.last_shot_time = world.resource::<Time>().elapsed_secs();
        let generation = world
            .resource::<PlayerMap>()
            .get(&self.id)
            .expect("experiment player missing from PlayerMap")
            .session
            .generation;
        self.to_server
            .send(ClientMessage::PortalShot(CPortalShot { generation, result }))?;
        self.advance()?;
        Ok(response)
    }

    pub fn fire(&mut self) -> Result<Value> {
        if let Some(reason) = self.ready(PowerUpKind::SingleShot) {
            return Ok(json!({"status": "rejected", "reason": reason}));
        }
        let world = self.server.world();
        let gameplay = world.resource::<GameplayConfig>();
        let shot = CProjectileShot {
            origin: self.eye()?.into(),
            face_yaw: self.yaw(),
            face_pitch: self.direction.y.clamp(-1.0, 1.0).asin(),
            pattern: 0,
        };
        let spawns = calculate_projectile_spawns(
            &shot.origin,
            shot.face_yaw,
            shot.face_pitch,
            shot.pattern,
            gameplay,
            world.resource::<CollisionWorld>(),
            &world.resource::<SwitchState>().open_fields,
            MuzzleCheck::Enforced,
        );
        self.last_shot_time = world.resource::<Time>().elapsed_secs();
        let mut ids = Vec::new();
        for spawn in spawns {
            let id = self.next_shot;
            self.next_shot += 1;
            ids.push(id);
            self.projectiles.push(Shot {
                id,
                position: spawn.position,
                motion: ProjectileMotion::new(
                    spawn.direction_yaw,
                    spawn.direction_pitch,
                    world.resource::<MapSettings>().movement.projectile_speed,
                    &gameplay.projectiles,
                ),
            });
        }
        if !ids.is_empty() {
            self.to_server.send(ClientMessage::ProjectileShot(shot))?;
        }
        for id in &ids {
            let shot = self
                .projectiles
                .iter()
                .find(|shot| shot.id == *id)
                .expect("new shot missing from the projectile list");
            self.record(json!({"kind": "projectile_spawned", "projectile": id,
                "position": point(shot.position), "velocity": shot.motion.velocity.to_array()}));
        }
        self.advance()?;
        Ok(if ids.is_empty() {
            json!({"status": "rejected", "reason": "muzzle_blocked"})
        } else {
            json!({"status": "fired", "projectiles": ids})
        })
    }

    // The runner observes the server at each completed tick without playback
    // delay. Movement and flight remain client code; normal reports carry their
    // results to the server. World access supplies observations and diagnostics.
    pub fn advance(&mut self) -> Result<()> {
        let world = self.server.world();
        let config = world.resource::<GameplayConfig>();
        let portals = PortalSet::rebuild(
            &world.resource::<PortalMap>().snapshot_portals(),
            world.resource::<CollisionWorld>(),
            world.resource::<Carriers>(),
            config.portals.size,
            config.player.physics(),
        );
        let (messages, mut events) = self.owner.step(
            world,
            self.id,
            &portals,
            &mut self.direction,
            std::mem::take(&mut self.jump_requested),
        );
        for message in messages {
            self.to_server.send(message)?;
        }
        let mut bodies = bodies(world);
        for (target, position, yaw, _) in &mut bodies {
            if matches!(target, HitTarget::Player { id, .. } if *id == self.id) {
                *position = self.owner.position;
                *yaw = self.owner.motion.face_yaw.0;
            }
        }
        let environment = ProjectileEnvironment {
            delta: world.resource::<NetworkConfig>().tick_duration(),
            gravity: world.resource::<MapSettings>().movement.gravity * config.projectiles.gravity_scale,
            collision_world: world.resource::<CollisionWorld>(),
            portals: &portals,
            open_fields: &world.resource::<SwitchState>().open_fields,
        };
        let shooter = |target: HitTarget| matches!(target, HitTarget::Player { id, .. } if id == self.id);
        let mut alive = Vec::new();
        for mut shot in std::mem::take(&mut self.projectiles) {
            let result = step_projectile(
                shot.position,
                &mut shot.motion,
                &environment,
                |motion, position| {
                    bodies
                        .iter()
                        .find(|(target, ..)| shooter(*target))
                        .is_some_and(|(_, pos, yaw, physics)| {
                            projectile_overlaps_character(motion, position, pos, *yaw, *physics)
                        })
                },
                |motion, position, delta| {
                    bodies
                        .iter()
                        .filter(|(target, ..)| motion.left_shooter || !shooter(*target))
                        .filter_map(|(target, pos, yaw, physics)| {
                            projectile_character_hit(position, motion, delta, pos, *yaw, *physics)
                                .map(|hit| (*target, hit))
                        })
                        .min_by(|(_, a), (_, b)| a.time_of_impact.total_cmp(&b.time_of_impact))
                },
            );
            for event in result.events {
                let mut event = match event {
                    ProjectileFlightEvent::Expired => json!({"kind": "projectile_expired"}),
                    ProjectileFlightEvent::Portal { entry, exit } => {
                        json!({"kind": "portal_crossing", "entry": entry.to_array(), "exit": exit.to_array()})
                    }
                    ProjectileFlightEvent::Bounce { bounce, velocity, .. } => {
                        json!({"kind": "bounce", "position": point(bounce.position), "normal": bounce.normal.to_array(), "velocity": velocity.to_array()})
                    }
                    ProjectileFlightEvent::Field { impact, .. } => {
                        json!({"kind": "field_impact", "field": impact.field.0, "position": impact.point.to_array()})
                    }
                    ProjectileFlightEvent::Hit {
                        target: victim,
                        hit,
                        position,
                        ..
                    } => {
                        self.to_server.send(ClientMessage::ProjectileHit(CProjectileHit {
                            target: victim,
                            direction: [hit.direction.x, hit.direction.z],
                        }))?;
                        json!({"kind": "hit_reported", "target": target(victim), "position": position.to_array()})
                    }
                };
                event["projectile"] = json!(shot.id);
                events.push(event);
            }
            shot.position = result.position;
            events.push(json!({"kind": "projectile_sample", "projectile": shot.id,
                "position": point(shot.position), "velocity": shot.motion.velocity.to_array(), "terminated": result.terminated}));
            if !result.terminated {
                alive.push(shot);
            }
        }
        self.projectiles = alive;
        self.server.update();
        for event in events {
            self.record(event);
        }
        self.receive()
    }

    fn receive(&mut self) -> Result<()> {
        while let Ok(message) = self.from_server.try_recv() {
            if let Some(messages) = &mut self.visual_messages {
                match &message {
                    ServerMessage::Firework(_) | ServerMessage::PressurePlate(_) | ServerMessage::PortalFizzled(_) => {
                        messages.push(message.clone())
                    }
                    _ => {}
                }
            }
            let event = match message {
                ServerMessage::PortalOpened(message) => json!({"kind": "portal_opened",
                    "portal": portal(message.portal, self.server.world().resource::<Carriers>())}),
                ServerMessage::PortalFizzled(_) => json!({"kind": "portal_fizzled"}),
                ServerMessage::Firework(_) => json!({"kind": "fireworks_started"}),
                ServerMessage::EquipmentErased(_) => json!({"kind": "equipment_erased"}),
                ServerMessage::PlayerStatus(status) if status.id == self.id && status.collected.is_some() => {
                    json!({"kind": "item_collected", "item": status.collected.expect("collected item missing from a pickup status").config_id()})
                }
                ServerMessage::ActorHit(hit) => json!({"kind": "actor_hit", "actor": hit.id.0, "health": hit.health.0}),
                ServerMessage::ActorDeath(death) => json!({"kind": "actor_died", "actor": death.id.0,
                    "killer": death.killer.map(|id| id.0)}),
                ServerMessage::PlayerHit(hit) => json!({"kind": "player_hit", "health": hit.health.0}),
                ServerMessage::PlayerFallDamage(hit) => json!({"kind": "player_fall_damage", "health": hit.health.0}),
                ServerMessage::CheckpointReached(message) => {
                    json!({"kind": "checkpoint_reached", "checkpoint": message.checkpoint})
                }
                ServerMessage::PlayerDeath(_) => json!({"kind": "player_died"}),
                ServerMessage::PlayerRelocated(message) if message.id == self.id => {
                    self.owner.relocate(&message.player);
                    self.last_shot_time = f32::NEG_INFINITY;
                    json!({"kind": "player_relocated"})
                }
                ServerMessage::PlayerKnockback(message)
                    if message.id == self.id && message.generation == self.owner.generation =>
                {
                    let impulse = Vec3::from_array(message.impulse);
                    self.owner.motion.vertical_velocity.0 += impulse.y;
                    let max = self
                        .server
                        .world()
                        .resource::<MapSettings>()
                        .movement
                        .knockback
                        .max_speed
                        * CHARACTER_KNOCKBACK_CLAMP_RATIO;
                    self.owner.motion.knockback.0 =
                        (self.owner.motion.knockback.0 + impulse.with_y(0.0)).clamp_length_max(max);
                    json!({"kind": "player_knockback", "impulse": message.impulse})
                }
                _ => continue,
            };
            self.record(event);
        }
        Ok(())
    }

    pub fn begin_move(&mut self, direction: [f32; 2], crouch: bool, jump: bool) {
        let heading = direction[0].atan2(direction[1]);
        let yaw = self.direction.x.atan2(self.direction.z);
        self.owner.motion.move_intent = if direction == [0.0, 0.0] {
            PlayerMoveIntent::NONE
        } else {
            PlayerMoveIntent {
                forward: (heading - yaw).cos(),
                sideways: (heading - yaw).sin(),
                yaw,
                pitch: self.direction.y.clamp(-1.0, 1.0).asin(),
                crouch,
            }
        };
        self.owner.motion.move_intent.crouch = crouch;
        if direction != [0.0, 0.0] {
            self.owner.motion.face_yaw.0 = heading;
        }
        self.jump_requested = jump;
    }

    pub fn end_move(&mut self) {
        self.owner.motion.move_intent = PlayerMoveIntent::NONE;
        self.jump_requested = false;
    }

    pub fn alive(&self) -> bool {
        !self
            .server
            .world()
            .resource::<PlayerMap>()
            .get(&self.id)
            .expect("experiment player missing from PlayerMap")
            .is_dead()
    }

    pub fn check(&self, min: [f32; 3], max: [f32; 3], grounded: bool) -> Value {
        let alive = !self
            .server
            .world()
            .resource::<PlayerMap>()
            .get(&self.id)
            .expect("experiment player missing from PlayerMap")
            .is_dead();
        let pos = point(self.owner.position);
        let inside = (0..3).all(|axis| pos[axis] >= min[axis] && pos[axis] <= max[axis]);
        // Transit follows the motor: that tick's support still describes the
        // entrance. Require a fresh motor result before claiming a landing.
        let on_ground = self.owner.motion.support == CharacterSupport::Ground && !self.owner.crossed_last_step;
        let reason = if !alive {
            Some("player_dead")
        } else if !inside {
            Some("outside_region")
        } else if grounded && !on_ground {
            Some("not_grounded")
        } else {
            None
        };
        json!({"status": if reason.is_none() { "passed" } else { "failed" }, "reason": reason,
            "inside": inside, "grounded": on_ground})
    }

    fn validate_initial_position(&self) -> Result<()> {
        let world = self.server.world();
        let player = world
            .resource::<PlayerMap>()
            .get(&self.id)
            .context("experiment player missing")?;
        if let Some(entity) = player.entity() {
            let position = world.get::<Position>(entity).context("player position missing")?;
            ensure!(!self.overlaps_geometry(position)?, "player overlaps geometry");
        }
        Ok(())
    }

    fn overlaps_geometry(&self, position: &Position) -> Result<bool> {
        let world = self.server.world();
        let player = world
            .resource::<PlayerMap>()
            .get(&self.id)
            .context("experiment player missing")?;
        let collision = world.resource::<CollisionWorld>();
        let physics = world.resource::<GameplayConfig>().player.physics();
        let passable = passable_fields(&player.life.held_keys, &world.resource::<SwitchState>().open_fields);
        Ok(collision.character_penetrates_solid(position, physics, &passable))
    }

    pub fn teleport(&mut self, feet: [f32; 3]) -> Result<Value> {
        let position = Position {
            x: feet[0],
            y: feet[1],
            z: feet[2],
        };
        if !self.alive() {
            return Ok(json!({"status": "rejected", "reason": "player_dead"}));
        }
        if self.overlaps_geometry(&position)? {
            return Ok(json!({"status": "rejected", "reason": "player overlaps geometry"}));
        }
        self.owner.teleport(position);
        Ok(json!({"status": "teleported"}))
    }
}

pub(super) fn yaw_of(direction: Vec3) -> f32 {
    direction.x.atan2(direction.z)
}

fn bodies(world: &World) -> Vec<(HitTarget, Position, f32, CharacterPhysicsConfig)> {
    let config = world.resource::<GameplayConfig>();
    let mut players: Vec<_> = world.resource::<PlayerMap>().iter().collect();
    players.sort_by_key(|(id, _)| id.0);
    let mut result: Vec<_> = players
        .into_iter()
        .filter_map(|(id, player)| {
            let entity = player.entity()?;
            Some((
                HitTarget::Player {
                    id: *id,
                    generation: player.session.generation,
                },
                *world.get::<Position>(entity)?,
                world.get::<FaceYaw>(entity)?.0,
                player.stance().physics(&config.player),
            ))
        })
        .collect();
    let mut actors: Vec<_> = world.resource::<ActorMap>().iter().collect();
    actors.sort_by_key(|(id, _)| id.0);
    result.extend(actors.into_iter().filter_map(|(id, actor)| {
        Some((
            HitTarget::Actor(*id),
            *world.get::<Position>(actor.entity)?,
            world.get::<FaceYaw>(actor.entity)?.0,
            config.expect_actor(&actor.spawn_kind).physics(),
        ))
    }));
    result
}
