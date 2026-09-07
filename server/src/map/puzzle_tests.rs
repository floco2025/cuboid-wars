use bevy::math::Vec3;
use common::{
    constants::TICK_SECS,
    map::Carriers,
    physics::{
        AirborneMomentum, CharacterSupport, CollisionWorld, PlayerMovementStep, PortalPlacement, PortalSet,
        compute_portal_placement, step_player_movement,
    },
    protocol::{BarrierKindId, BridgeKindId, CarrierId, PlayerMoveIntent, PortalEnd, PortalPairId, Position},
};

use super::{GeneratedMap, generate_map};
use crate::config::{MapServerConfig, ServerGameplayConfig};

struct Puzzle {
    server: ServerGameplayConfig,
    settings: MapServerConfig,
    map: GeneratedMap,
    world: CollisionWorld,
    carriers: Carriers,
}

impl Puzzle {
    fn load(name: &str) -> Self {
        let server = ServerGameplayConfig::load_default().expect("gameplay config rejected");
        let settings = server.maps[name].clone();
        let (barriers, bridges) = settings.settings.kind_tables().expect("puzzle kind catalogs rejected");
        let map = generate_map(name, &settings.settings, &barriers, &bridges).expect("puzzle map rejected");
        let world = CollisionWorld::from_map_layout(&map.layout, &barriers);
        let carriers = Carriers::from_layout(&map.layout);
        Self {
            server,
            settings,
            map,
            world,
            carriers,
        }
    }

    fn point(&self, col: f32, row: f32, y: f32) -> Vec3 {
        let g = self.map.config.root_grid().geometry;
        Vec3::new(
            g.cell_to_world_x(0) + col * g.cell_size(),
            y,
            g.cell_to_world_z(0) + row * g.cell_size(),
        )
    }

    fn portal(&self, from: Vec3, to: Vec3, open: &[BarrierKindId]) -> Option<PortalPlacement> {
        compute_portal_placement(
            from,
            to - from,
            0.0,
            100.0,
            &self.world,
            &self.map.layout,
            &self.carriers,
            open,
            &self.settings.settings.textures,
        )
        .ok()
    }

    fn turret_samples(&self, col: f32, row: f32, level: u8) -> Vec<Vec3> {
        let y = self.settings.settings.geometry.level_y(level)
            + self
                .server
                .gameplay_config()
                .expect_actor("turret")
                .physics()
                .collider_center_y(0.0);
        [0.04, 0.5, 0.96]
            .into_iter()
            .flat_map(|x| {
                [0.04, 0.5, 0.96]
                    .into_iter()
                    .map(move |z| self.point(col + x, row + z, y))
            })
            .collect()
    }
}

#[test]
fn stages_has_a_clear_missile_peek_and_a_sheltered_resupply() {
    let p = Puzzle::load("puzzle_stages");
    for turret in p.turret_samples(13.0, 3.0, 0) {
        assert!(p.world.attack_path_clear(turret, p.point(9.5, 3.5, 1.15), &[]));
        for row in [4.5, 5.5] {
            assert!(!p.world.attack_path_clear(turret, p.point(8.5, row, 1.15), &[]));
        }
    }
}

#[test]
fn access_keeps_the_key_route_shielded_while_opening_exposes_the_switch() {
    let p = Puzzle::load("puzzle_access");
    for turret in p.turret_samples(8.0, 2.0, 0) {
        let switch = p.point(2.5, 2.5, 1.15);
        assert!(!p.world.attack_path_clear(turret, switch, &[]));
        assert!(p.world.attack_path_clear(turret, switch, &[BarrierKindId(0)]));
        assert!(!p.world.attack_path_clear(turret, p.point(5.5, 4.5, 1.15), &[]));
    }
}

#[test]
fn roof_bridge_blocks_the_lower_beam_and_upper_wall_shields_the_return() {
    let mut p = Puzzle::load("puzzle_shield");
    for turret in p.turret_samples(9.0, 4.0, 1) {
        let lower = p.point(4.5, 4.5, 1.15);
        assert!(
            p.world.attack_path_clear(turret, lower, &[]),
            "unpowered roof already shields {turret}"
        );
        p.world.set_powered_bridges(&[BridgeKindId(0)]);
        assert!(!p.world.attack_path_clear(turret, lower, &[]));
        for col in [2.5, 4.5, 6.5] {
            assert!(!p.world.attack_path_clear(turret, p.point(col, 4.5, 5.15), &[]));
        }
        p.world.set_powered_bridges(&[]);
    }
}

#[test]
fn shuttle_wall_protects_passengers_throughout_its_journey() {
    let mut p = Puzzle::load("puzzle_cover");
    for tick in (0..780).step_by(15) {
        p.carriers.advance(tick);
        p.world.set_carrier_poses(&p.carriers);
        for x in [-2.0, 0.0, 2.0] {
            let passenger = p.carriers.pose(CarrierId(1)).transform_point(Vec3::new(x, 1.15, 2.0));
            for turret in p.turret_samples(7.0, 1.0, 1) {
                assert!(
                    !p.world.attack_path_clear(turret, passenger, &[]),
                    "unshielded at tick {tick}: {passenger}"
                );
            }
        }
    }
}

#[test]
fn sequence_exit_can_be_placed_through_the_open_shutter_and_is_sheltered() {
    let p = Puzzle::load("puzzle_sequence");
    let from = p.point(4.5, 3.5, 1.62);
    let to = p.point(10.5, 4.5, 0.0);
    assert!(p.portal(from, to, &[]).is_none());
    let exit = p
        .portal(from, to, &[BarrierKindId(0)])
        .expect("exit panel cannot be shot through shutter");
    assert!(exit.normal.abs_diff_eq(Vec3::Y, 0.01));
    assert!(p.portal(p.point(3.5, 3.5, 1.62), p.point(3.5, 3.5, 0.0), &[]).is_some());
    for turret in p.turret_samples(9.0, 2.0, 0) {
        assert!(!p.world.attack_path_clear(turret, to.with_y(1.15), &[BarrierKindId(0)]));
    }
}

#[test]
fn coop_crosser_can_place_both_portals_while_the_helper_holds_the_bridge() {
    let mut p = Puzzle::load("puzzle_coop");
    p.world.set_powered_bridges(&[BridgeKindId(0)]);
    let from = p.point(10.5, 3.5, 5.62);
    let entry = p
        .portal(from, p.point(1.0, 3.5, 5.62), &[])
        .expect("helper's portal panel is obscured");
    assert!(entry.normal.abs_diff_eq(Vec3::X, 0.01));
    assert!(p.portal(from, p.point(10.5, 3.5, 4.0), &[]).is_some());
}

#[test]
fn momentum_fall_reaches_the_landing_with_the_actual_player_motor() {
    let p = Puzzle::load("puzzle_momentum");
    let entry = p
        .portal(p.point(2.5, 4.5, 1.62), p.point(2.5, 4.5, 0.0), &[])
        .expect("drop panel rejected");
    let exit = p
        .portal(p.point(6.5, 4.5, 1.62), p.point(5.0, 4.5, 9.6), &[])
        .expect("launch panel rejected");
    assert!(exit.normal.abs_diff_eq(Vec3::X, 0.01));
    let portals = PortalSet::rebuild(
        &[
            entry.portal(PortalPairId(1), PortalEnd::A, &p.carriers),
            exit.portal(PortalPairId(1), PortalEnd::B, &p.carriers),
        ],
        &p.world,
        &p.carriers,
    );
    let gameplay = p.server.gameplay_config();
    let settings = &p.settings.settings;
    let mut position: Position = p.point(2.5, 4.5, 8.0).into();
    let mut vertical_velocity = 0.0;
    let mut momentum = AirborneMomentum::default();
    let mut hopped = false;
    for _ in 0..180 {
        let start = position;
        let movement = step_player_movement(PlayerMovementStep {
            start,
            vertical_velocity,
            control_velocity: Vec3::ZERO,
            additional_displacement: Vec3::ZERO,
            delta: TICK_SECS,
            has_low_gravity: false,
            held_keys: &[],
            open_kinds: &[],
            knockback: None,
            airborne_momentum: Some(&mut momentum),
            collision_world: &p.world,
            map_settings: settings,
            gameplay_config: &gameplay,
            portal_set: &portals,
            carriers: &p.carriers,
        });
        position = movement.position;
        vertical_velocity = movement.vertical_velocity;
        if let Some(hop) = portals.player_hop(
            start.into(),
            position.into(),
            &gameplay,
            &settings.movement,
            PlayerMoveIntent::Idle,
            false,
            false,
            None,
            Some(&momentum),
            vertical_velocity,
            0.0,
        ) {
            position = hop.origin.into();
            vertical_velocity = hop.vertical_velocity;
            momentum.0 = hop.portal_momentum;
            hopped = true;
        } else if hopped && movement.support == CharacterSupport::Ground {
            assert!(
                (position.y - 4.0).abs() < 0.05,
                "missed the raised landing: {position:?}"
            );
            assert!(position.x >= p.point(8.0, 4.5, 0.0).x - 0.2);
            return;
        }
    }
    panic!("fling did not land: {position:?}, hopped: {hopped}");
}
