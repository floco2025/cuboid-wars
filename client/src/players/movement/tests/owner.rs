use super::*;
use crate::test_fixtures::{self, portal};
use common::{
    config::MapMovementConfig,
    protocol::{Floor, MapLayout, Portal, PortalEnd, SwitchState},
};

struct Harness {
    entity: Entity,
    collision: CollisionWorld,
    carriers: Carriers,
    settings: MapSettings,
    gameplay: GameplayConfig,
    portals: PortalSet,
    network: NetworkConfig,
    position: Position,
    previous_position: Position,
    intent: PlayerMoveIntent,
    face_yaw: FaceYaw,
    vertical_velocity: CharacterVerticalVelocity,
    horizontal_velocity: HorizontalVelocity,
    knockback: KnockbackVelocity,
    stance: PlayerStance,
    support: CharacterSupport,
    jump: JumpRequest,
    step: LocalMovementStep,
    reports: LocalMovementReports,
    stunned: bool,
}

impl Harness {
    fn new(layout: MapLayout, portals: &[Portal]) -> Self {
        let collision = CollisionWorld::from_map_layout(&layout);
        let carriers = Carriers::from_layout(&layout);
        let gameplay = test_fixtures::gameplay_config();
        let portals = PortalSet::rebuild(
            portals,
            &collision,
            &carriers,
            gameplay.portals.size,
            gameplay.player.physics(),
        );
        Self {
            entity: Entity::from_raw_u32(1).expect("entity"),
            collision,
            carriers,
            settings: test_fixtures::map_settings(),
            gameplay,
            portals,
            network: NetworkConfig {
                update_hz: 10,
                ..default()
            },
            position: Position::default(),
            previous_position: Position::default(),
            intent: PlayerMoveIntent::NONE,
            face_yaw: FaceYaw(0.0),
            vertical_velocity: CharacterVerticalVelocity(0.0),
            horizontal_velocity: HorizontalVelocity::default(),
            knockback: KnockbackVelocity::default(),
            stance: PlayerStance::default(),
            support: CharacterSupport::Airborne,
            jump: JumpRequest::default(),
            step: LocalMovementStep::default(),
            reports: LocalMovementReports::default(),
            stunned: false,
        }
    }

    fn movement(&mut self) -> &mut MapMovementConfig {
        &mut self.settings.movement
    }

    fn tick(&mut self) -> OwnerTickOutcome {
        let world = OwnerWorld {
            collision_world: &self.collision,
            carriers: &self.carriers,
            map_settings: &self.settings,
            gameplay_config: &self.gameplay,
            portal_set: &self.portals,
            network: &self.network,
            open_fields: &SwitchState::default().open_fields,
            held_keys: &[],
            has_speed: false,
            has_low_gravity: false,
            stunned: self.stunned,
            delta: 1.0 / 30.0,
        };
        owner_tick(
            self.entity,
            OwnerBody {
                position: &mut self.position,
                previous_position: &mut self.previous_position,
                intent: &mut self.intent,
                face_yaw: &mut self.face_yaw,
                vertical_velocity: &mut self.vertical_velocity,
                horizontal_velocity: &mut self.horizontal_velocity,
                knockback: &mut self.knockback,
                stance: &mut self.stance,
                support: &mut self.support,
                jump: &mut self.jump,
                step: &mut self.step,
                reports: &mut self.reports,
            },
            &world,
            &[],
        )
    }
}

fn floor() -> MapLayout {
    MapLayout {
        floors: vec![Floor {
            x1: -5.0,
            x2: 5.0,
            z1: -5.0,
            z2: 5.0,
            y: 0.0,
            thickness: 0.2,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..default()
    }
}

fn wall_pair() -> [Portal; 2] {
    [
        portal(PortalEnd::A, Vec3::new(0.0, 1.6, 0.0), Vec3::Z),
        portal(PortalEnd::B, Vec3::new(10.0, 1.6, 0.0), Vec3::Z),
    ]
}

// Whether a press is waiting for a tick to spend it.
fn pending(jump: &JumpRequest) -> bool {
    jump.pressed || jump.buffered_secs.is_some()
}

#[test]
fn a_tick_moves_the_body_records_its_step_and_reports_on_the_cadence() {
    let mut owner = Harness::new(floor(), &[]);
    owner.position.y = 1.0;
    owner.vertical_velocity.0 = -3.0;
    owner.horizontal_velocity.0 = Vec3::X;
    let outcome = owner.tick();
    assert!(owner.position.y < 1.0 && owner.position.x > 0.0);
    assert_eq!(owner.previous_position, Position { y: 1.0, ..default() });
    assert_eq!(owner.step.start, owner.previous_position);
    assert_eq!(owner.step.result.position, owner.position);
    assert_eq!(owner.support, CharacterSupport::Airborne);
    let report = outcome.report.expect("the first report is due at once");
    assert_eq!(report.movement.pos, owner.position);
    assert!(outcome.hop.is_none() && outcome.jump.is_none() && outcome.outcomes.is_empty());
    assert!(owner.tick().report.is_none(), "the cadence holds off the next report");
}

#[test]
fn a_jump_request_fires_once_unless_crouched_or_stunned() {
    let mut owner = Harness::new(floor(), &[]);
    owner.movement().player.jump_speed = 12.0;
    owner.tick();
    assert_eq!(owner.support, CharacterSupport::Ground);
    owner.jump.pressed = true;
    owner.stance.crouched = true;
    assert!(owner.tick().jump.is_none(), "a crouched body cannot jump");
    assert!(!pending(&owner.jump), "a crouched press is dropped");
    owner.stance.crouched = false;
    owner.intent.crouch = false;
    owner.tick();
    owner.jump.pressed = true;
    owner.stunned = true;
    assert!(owner.tick().jump.is_none(), "a stunned body cannot jump");
    assert!(!pending(&owner.jump), "a stunned press is dropped");
    owner.stunned = false;
    owner.tick();
    owner.jump.pressed = true;
    let jumped = owner.tick();
    assert_eq!(jumped.jump, Some(PlayerJump::Rise(12.0)));
    assert!(owner.vertical_velocity.0 > 10.0);
    assert!(owner.tick().jump.is_none(), "one request is one jump");
}

#[test]
fn a_press_just_before_landing_jumps_on_the_landing_tick() {
    let mut owner = Harness::new(floor(), &[]);
    owner.movement().player.jump_speed = 12.0;
    owner.position.y = 0.3;
    owner.vertical_velocity.0 = -6.0;
    let mut ticks = 0;
    for _ in 0..3 {
        let outcome = owner.tick();
        ticks += 1;
        if owner.support == CharacterSupport::Ground {
            assert!(outcome.jump.is_none());
            break;
        }
        owner.jump.pressed = true;
    }
    assert!(ticks > 1 && ticks <= 3, "the fall should take a tick or two: {ticks}");
    assert!(pending(&owner.jump), "the press waits for the landing");
    let landing = owner.tick();
    assert_eq!(landing.jump, Some(PlayerJump::Rise(12.0)));
    assert!(!pending(&owner.jump));
}

#[test]
fn a_press_within_the_coyote_window_still_jumps_off_the_edge() {
    let mut owner = Harness::new(floor(), &[]);
    owner.movement().player.jump_speed = 12.0;
    owner.movement().player.air_deceleration = 0.0;
    owner.tick();
    assert_eq!(owner.support, CharacterSupport::Ground);
    // Walk off the floor's edge at +X.
    owner.position.x = 4.9;
    owner.horizontal_velocity.0 = Vec3::X * 6.0;
    while owner.support == CharacterSupport::Ground {
        owner.tick();
        assert!(owner.position.x < 6.0, "never left the floor");
    }
    owner.tick();
    owner.jump.pressed = true;
    let late = owner.tick();
    assert_eq!(
        late.jump,
        Some(PlayerJump::Rise(12.0)),
        "two ticks past the edge is inside the window"
    );
    for _ in 0..8 {
        owner.tick();
    }
    assert!(owner.vertical_velocity.0 < 12.0 && owner.position.y > 0.0);
    owner.jump.pressed = true;
    let stale = owner.tick();
    assert!(stale.jump.is_none(), "well past the edge, and rising, there is no jump");
    let mut late_owner = Harness::new(floor(), &[]);
    late_owner.movement().player.jump_speed = 12.0;
    late_owner.tick();
    late_owner.position.x = 4.9;
    late_owner.horizontal_velocity.0 = Vec3::X * 6.0;
    while late_owner.support == CharacterSupport::Ground {
        late_owner.tick();
    }
    for _ in 0..4 {
        late_owner.tick();
    }
    late_owner.jump.pressed = true;
    assert!(
        late_owner.tick().jump.is_none(),
        "five ticks past the edge is outside the window"
    );
}

#[test]
fn a_crossing_continues_from_the_exit_maps_the_motion_and_reports_at_once() {
    let mut owner = Harness::new(MapLayout::default(), &wall_pair());
    owner.movement().player.air_deceleration = 0.0;
    owner.position = Position {
        x: 0.0,
        y: 0.7,
        z: 0.05,
    };
    owner.horizontal_velocity.0 = Vec3::NEG_Z * 4.0;
    owner.knockback.0 = Vec3::NEG_Z * 2.0;
    owner.face_yaw.0 = 0.0;
    let outcome = owner.tick();
    let hop = outcome.hop.expect("the body crossed the pair");
    assert!((hop.entry.center.x).abs() < 1e-5 && (hop.exit.center.x - 10.0).abs() < 1e-5);
    assert!(
        (owner.position.x - 10.0).abs() < 1e-4 && owner.position.z > 0.0,
        "{:?}",
        owner.position
    );
    assert_eq!(
        owner.previous_position, owner.position,
        "the render lerp cuts at the exit"
    );
    assert!(
        (owner.horizontal_velocity.0.z - 4.0).abs() < 1e-3,
        "{:?}",
        owner.horizontal_velocity
    );
    assert!(owner.knockback.0.z > 0.0, "{:?}", owner.knockback);
    let report = outcome.report.expect("a crossing reports at once");
    assert_eq!((report.seq, report.portal_crossing), (1, 1));
    assert_eq!(report.movement.carrier, CarrierId::WORLD);
    assert!((report.movement.pos.x - 10.0).abs() < 1e-4);
    assert!(outcome.outcomes.is_empty(), "a crossing is not a landing");
}

#[test]
fn a_held_climber_faces_the_ladder() {
    use common::protocol::Ladder;
    for normal in [Vec3::X, Vec3::NEG_Z] {
        let collision = CollisionWorld::from_map_layout(&MapLayout {
            ladders: vec![Ladder {
                x1: -normal.z * 0.5,
                z1: normal.x * 0.5,
                x2: normal.z * 0.5,
                z2: -normal.x * 0.5,
                nx: normal.x,
                nz: normal.z,
                y: 0.0,
                height: 4.0,
                level: 0,
                levels: 1,
                carrier: CarrierId::WORLD,
            }],
            ..default()
        });
        let front = Position {
            x: normal.x * 0.4,
            y: 1.0,
            z: normal.z * 0.4,
        };
        let yaw = ladder_facing(&collision, &front, CharacterSupport::Ladder).expect("held climber faces the ladder");
        assert!(Vec3::new(yaw.sin(), 0.0, yaw.cos()).dot(-normal) > 0.9999);
        assert_eq!(ladder_facing(&collision, &front, CharacterSupport::Airborne), None);
    }
}
