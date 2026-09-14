use super::*;
use crate::test_fixtures;

fn state(sun_altitude_degrees: f32) -> SkyState {
    let altitude = sun_altitude_degrees.to_radians();
    let daylight = smoothstep(-4.0_f32.to_radians(), 10.0_f32.to_radians(), altitude);
    SkyState {
        sun_direction: Vec3::new(altitude.cos(), altitude.sin(), 0.0),
        sun_altitude: altitude,
        twilight: smoothstep(-12.0_f32.to_radians(), -3.0_f32.to_radians(), altitude),
        daylight,
        rain: 0.0,
        sun_illuminance: 6000.0 * daylight,
        ambient_brightness: 15.0f32.lerp(70.0, daylight),
        seconds: 100.0,
    }
}

#[test]
fn face_texels_point_out_of_their_faces() {
    let centre = PROBE_SIZE / 2;
    for (face, axis) in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z]
        .into_iter()
        .enumerate()
    {
        let direction = face_direction(face as u32, centre, centre);
        assert!(direction.dot(axis) > 0.99, "face {face} centre points along {axis}");
        for (u, v) in [
            (0, 0),
            (PROBE_SIZE - 1, 0),
            (0, PROBE_SIZE - 1),
            (PROBE_SIZE - 1, PROBE_SIZE - 1),
        ] {
            let corner = face_direction(face as u32, u, v);
            assert!((corner.length() - 1.0).abs() < 1e-5);
            assert!(corner.dot(axis) > 0.5, "face {face} corners stay on their face");
        }
    }
    // Rows run down the face: the top row of +Z looks up.
    assert!(face_direction(4, centre, 0).y > 0.0);
}

#[test]
fn day_sky_outshines_night_and_the_ground_reflects_the_meadow() {
    let sky = test_fixtures::client_settings().sky;
    let up = Vec3::Y;
    let day = sky_radiance(up, &state(40.0), sky);
    let night = sky_radiance(up, &state(-30.0), sky);
    assert!(day.length() > night.length() * 5.0);
    assert!(day.z > day.x, "the day zenith is blue");

    let texels = probe_texels(&state(40.0), sky, Color::srgb(0.35, 0.41, 0.19));
    assert_eq!(texels.len(), (FACES * PROBE_SIZE * PROBE_SIZE * 8) as usize);
    let texel = |face: u32, u: u32, v: u32| {
        let offset = (((face * PROBE_SIZE + v) * PROBE_SIZE + u) * 8) as usize;
        let channel = |i: usize| f16::from_le_bytes([texels[offset + i * 2], texels[offset + i * 2 + 1]]).to_f32();
        Vec3::new(channel(0), channel(1), channel(2))
    };
    let ground = texel(3, PROBE_SIZE / 2, PROBE_SIZE / 2);
    assert!(
        ground.y > ground.x && ground.y > ground.z,
        "the bottom face carries the meadow's green"
    );
    assert!(ground.y > 0.0);
    // The upper hemisphere's mean luminance is the configured ambient level.
    let mut total = 0.0;
    let mut count = 0.0;
    for face in 0..FACES {
        for v in 0..PROBE_SIZE {
            for u in 0..PROBE_SIZE {
                if face_direction(face, u, v).y > 0.0 {
                    total += texel(face, u, v).dot(LUMINANCE);
                    count += 1.0;
                }
            }
        }
    }
    assert!((total / count - 70.0).abs() < 1.0);
}
