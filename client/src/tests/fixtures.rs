use std::collections::HashMap;

use common::{
    celestial::{CelestialMapSettings, LocalTime, Season},
    config::{GameplayConfig, KnockbackConfig, MapGeometryConfig, MapMovementConfig, PlayerMovementConfig},
    map::MapGeometry,
    physics::{CollisionWorld, PortalFrame, Solid},
    protocol::{
        Carrier, CarrierId, CarrierMotion, MapLayout, MapSettings, Portal, PortalEnd, PortalMode, PortalPairId,
        Position,
    },
};

use bevy::{audio::GlobalVolume, prelude::*};

use crate::{
    audio::{LoopAudio, LowPassAudio},
    config::{AssetSet, ClientSettings, FollowCameraConfig},
};

// The shipped settings are the base of every whole-schema `ClientSettings`;
// each test pins the values its assertions depend on.
pub(crate) const SETTINGS_JSON: &str = include_str!("../../../config/client/client.json");
pub(crate) const ASSETS_JSON: &str = include_str!("fixtures/assets.json");

pub(crate) fn gameplay_config() -> GameplayConfig {
    serde_json::from_str(include_str!("../../../common/src/config/tests/fixtures/gameplay.json"))
        .expect("test gameplay config is invalid")
}

pub(crate) fn client_settings() -> ClientSettings {
    serde_json::from_str(SETTINGS_JSON).expect("test client settings are invalid")
}

// Every source type and resource the audio plugin's systems query.
pub(crate) fn init_audio_app(app: &mut App) {
    app.init_resource::<GlobalVolume>()
        .init_asset::<AudioSource>()
        .init_asset::<LoopAudio>()
        .init_asset::<LowPassAudio<AudioSource>>()
        .init_asset::<LowPassAudio<LoopAudio>>();
}

pub(crate) fn asset_set() -> AssetSet {
    serde_json::from_str(ASSETS_JSON).expect("test asset configuration is invalid")
}

pub(crate) const CELL: f32 = 3.4;
pub(crate) const LEVEL_HEIGHT: f32 = 4.4;
pub(crate) const FLOOR_THICKNESS: f32 = 0.4;
pub(crate) const WALL_THICKNESS: f32 = 0.3;
pub(crate) const WALL_HEIGHT: f32 = LEVEL_HEIGHT - FLOOR_THICKNESS;

pub(crate) fn geometry(cols: i32, rows: i32) -> MapGeometry {
    MapGeometry::new(cols, rows, sizes())
}

pub(crate) fn sizes() -> MapGeometryConfig {
    MapGeometryConfig {
        grid_cell_size: CELL,
        level_height: LEVEL_HEIGHT,
        floor_thickness: FLOOR_THICKNESS,
        wall_thickness: WALL_THICKNESS,
    }
}

// The settings resource for systems that only read `geometry`.
pub(crate) fn map_settings() -> MapSettings {
    MapSettings {
        grounds: None,
        celestial: CelestialMapSettings {
            latitude_degrees: 40.0,
            season: Season::Summer,
            north_yaw_degrees: 0.0,
            start_local_time: LocalTime::parse("09:00").expect("valid fixture time"),
            start_moon_phase: 0.25,
        },
        textures: Default::default(),

        geometry: sizes(),
        movement: MapMovementConfig {
            player: PlayerMovementConfig {
                move_speed: 4.0,
                move_speed_power_up: 1.5,
                move_speed_ladder: 0.4,
                jump_speed: 12.0,
                ground_acceleration: 40.0,
                ground_deceleration: 16.0,
                ground_lateral_deceleration: 40.0,
                air_acceleration: 5.0,
                air_deceleration: 0.0,
                air_lateral_deceleration: 0.0,
            },
            actors: HashMap::new(),
            missile_speed: 20.0,
            projectile_speed: 30.0,
            gravity: 20.0,
            low_gravity: 5.0,
            knockback: KnockbackConfig {
                max_speed: 10.0,
                up_speed: 4.0,
                deceleration: 12.0,
            },
        },
        portals: PortalMode::Both,
        switches: Vec::new(),
        fields: Vec::new(),
    }
}

// A follow camera with room to zoom, so arm tests are not bound to the
// shipped `max_distance`.
pub(crate) fn follow_camera() -> FollowCameraConfig {
    FollowCameraConfig {
        max_distance: 6.0,
        first_person_distance: 0.7,
        pivot_height: 1.4,
        shoulder_offset: 0.0,
        collision_radius: 0.2,
        obstruction_return_rate: 8.0,
    }
}

pub(crate) const PORTAL_HALF_WIDTH: f32 = 0.7;
pub(crate) const PORTAL_HALF_HEIGHT: f32 = 1.3;

// One end of pair 1 on the static world, facing `normal`.
pub(crate) fn portal(end: PortalEnd, center: Vec3, normal: Vec3) -> Portal {
    Portal {
        pair: PortalPairId(1),
        end,
        pos: center.into(),
        nx: normal.x,
        ny: normal.y,
        nz: normal.z,
        yaw: 0.0,
        carrier: CarrierId::WORLD,
    }
}

// An aperture of the test gameplay's portal size on a surface facing `normal`.
pub(crate) fn portal_frame(center: Vec3, normal: Vec3) -> PortalFrame {
    PortalFrame::from_surface(center, normal, 0.0, gameplay_config().portals.size)
}

// The structural solids of a layout, for code that reads the collision
// world's geometry. Records may name carriers the layout does not list:
// each gets one parked at the origin.
pub(crate) fn structural_solids(layout: &MapLayout) -> Vec<Solid> {
    let mut layout = layout.clone();
    let highest = layout
        .walls
        .iter()
        .map(|wall| wall.carrier)
        .chain(layout.floors.iter().map(|floor| floor.carrier))
        .chain(layout.ramps.iter().map(|ramp| ramp.carrier))
        .chain(layout.barriers.iter().map(|barrier| barrier.carrier))
        .chain(layout.light_bridges.iter().map(|bridge| bridge.carrier))
        .map(|carrier| usize::from(carrier.0))
        .max()
        .unwrap_or(0);
    while layout.carriers.len() < highest {
        layout.carriers.push(Carrier {
            motion: CarrierMotion::default(),
            initially_on: true,
            parent: CarrierId::WORLD,
            level: 0,
            levels: 0,
            from: Position::default(),
            to: Position::default(),
            travel_ticks: 60,
            pause_ticks: 0,
            phase_ticks: 0,
            switch: None,
        });
    }
    CollisionWorld::from_map_layout(&layout).structural_solids()
}
