#import bevy_pbr::{
    forward_io::VertexOutput,
    mesh_view_bindings::view,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> sun_direction: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> moon_direction: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> pole_rotation: vec4<f32>;
// x real time, y rain, z illuminated fraction, w waxing (+1) / waning (-1)
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> time_weather_phase: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var<uniform> day_horizon: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var<uniform> day_zenith: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var<uniform> sunset: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var<uniform> twilight_horizon: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var<uniform> twilight_zenith: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(9) var<uniform> night_horizon: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(10) var<uniform> night_zenith: vec4<f32>;
// angular radius, core luminance, halo radius, halo luminance
@group(#{MATERIAL_BIND_GROUP}) @binding(11) var<uniform> sun: vec4<f32>;
// angular radius, luminance, earthshine, crater contrast
@group(#{MATERIAL_BIND_GROUP}) @binding(12) var<uniform> moon: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(13) var<uniform> moon_halo: vec4<f32>;
// seed, density, min luminance, max luminance
@group(#{MATERIAL_BIND_GROUP}) @binding(14) var<uniform> stars: vec4<f32>;
// bright fraction, twinkle
@group(#{MATERIAL_BIND_GROUP}) @binding(15) var<uniform> star_detail: vec4<f32>;
// clear coverage, overcast coverage, scale
@group(#{MATERIAL_BIND_GROUP}) @binding(16) var<uniform> clouds: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(17) var<uniform> cloud_color: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(18) var<uniform> overcast_color: vec4<f32>;

const PI: f32 = 3.14159265359;

fn hash13(p: vec3<f32>) -> f32 {
    return fract(sin(dot(p, vec3(127.1, 311.7, 74.7))) * 43758.5453123);
}

fn value_noise(p: vec3<f32>) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let x00 = mix(hash13(cell), hash13(cell + vec3(1.0, 0.0, 0.0)), u.x);
    let x10 = mix(hash13(cell + vec3(0.0, 1.0, 0.0)), hash13(cell + vec3(1.0, 1.0, 0.0)), u.x);
    let x01 = mix(hash13(cell + vec3(0.0, 0.0, 1.0)), hash13(cell + vec3(1.0, 0.0, 1.0)), u.x);
    let x11 = mix(hash13(cell + vec3(0.0, 1.0, 1.0)), hash13(cell + vec3(1.0, 1.0, 1.0)), u.x);
    return mix(mix(x00, x10, u.y), mix(x01, x11, u.y), u.z);
}

fn cloud_noise(direction: vec3<f32>) -> f32 {
    let p = direction * clouds.z + vec3(13.7, 2.1, -8.4);
    return value_noise(p) * 0.55 + value_noise(p * 2.03) * 0.29 + value_noise(p * 4.11) * 0.16;
}

fn rotate_about_axis(v: vec3<f32>, axis: vec3<f32>, angle: f32) -> vec3<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return v * c + cross(axis, v) * s + axis * dot(axis, v) * (1.0 - c);
}

fn star_layer(direction: vec3<f32>, scale: f32, seed_offset: f32) -> vec3<f32> {
    let p = direction * scale;
    let cell = floor(p);
    let local = fract(p) - 0.5;
    let seed = stars.x + seed_offset;
    let jitter = vec3(
        hash13(cell + vec3(seed, 0.0, 0.0)),
        hash13(cell + vec3(0.0, seed, 0.0)),
        hash13(cell + vec3(0.0, 0.0, seed))
    ) - 0.5;
    let distance = length(local - jitter * 0.72);
    let selection = hash13(cell + vec3(seed * 0.37));
    let selected = step(1.0 - stars.y, selection);
    let core = smoothstep(0.075, 0.008, distance) * selected;
    let value = hash13(cell + vec3(seed * 1.91));
    var luminance = mix(stars.z, stars.w, value * value);
    luminance *= mix(1.0, 2.8, step(1.0 - star_detail.x, value));
    let twinkle = 1.0 + star_detail.y * sin(time_weather_phase.x * (0.7 + value * 1.8) + value * 31.0);
    let tint = mix(vec3(0.68, 0.79, 1.0), vec3(1.0, 0.88, 0.68), hash13(cell + vec3(seed * 2.7)));
    return tint * core * luminance * twinkle;
}

fn star_field(ray: vec3<f32>, night: f32) -> vec3<f32> {
    let rotated = rotate_about_axis(ray, normalize(pole_rotation.xyz), -pole_rotation.w);
    let field = star_layer(rotated, 185.0, 17.0) + star_layer(rotated, 317.0, 83.0) * 0.7;
    return field * night * smoothstep(-0.08, 0.12, ray.y);
}

fn moon_color(ray: vec3<f32>) -> vec3<f32> {
    let center = normalize(moon_direction.xyz);
    let separation = acos(clamp(dot(ray, center), -1.0, 1.0));
    let halo = exp(-pow(separation / max(moon_halo.x, 0.0001), 1.45)) * moon_halo.y
        * time_weather_phase.z;
    var result = vec3(0.72, 0.82, 1.0) * halo;
    if separation >= moon.x {
        return result;
    }

    var right = cross(vec3(0.0, 1.0, 0.0), center);
    if length(right) < 0.01 {
        right = vec3(1.0, 0.0, 0.0);
    } else {
        right = normalize(right);
    }
    let up = normalize(cross(center, right));
    let radius = sin(moon.x);
    let x = dot(ray, right) / radius;
    let y = dot(ray, up) / radius;
    let z = sqrt(max(0.0, 1.0 - x * x - y * y));
    let surface_normal = normalize(right * x + up * y - center * z);
    let direct = smoothstep(-0.018, 0.018, dot(surface_normal, normalize(sun_direction.xyz)));
    let crater_a = value_noise(surface_normal * 19.0 + vec3(4.0, 11.0, 2.0));
    let crater_b = value_noise(surface_normal * 47.0 + vec3(9.0, 3.0, 17.0));
    let relief = 1.0 + ((crater_a * 0.7 + crater_b * 0.3) - 0.5) * moon.w;
    let earthshine = moon.z * (0.45 + 0.55 * max(0.0, -surface_normal.y));
    // The direct sun vector establishes the physical terminator; this tiny
    // signed bias keeps the expected waxing/waning side stable at numerical
    // new/full phase where the projected vector degenerates.
    let orientation_bias = time_weather_phase.w * x * 0.002;
    let surface_light = max(earthshine, direct + orientation_bias);
    let limb = smoothstep(0.0, 0.12, z);
    result += vec3(0.86, 0.91, 1.0) * moon.y * relief * surface_light * limb;
    return result;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let ray = normalize(in.world_position.xyz - view.world_position);
    let elevation = clamp(ray.y, 0.0, 1.0);
    let gradient = pow(elevation, 0.42);
    let sun_dir = normalize(sun_direction.xyz);
    let sun_altitude = asin(clamp(sun_dir.y, -1.0, 1.0));
    let daylight = smoothstep(-0.075, 0.16, sun_altitude);
    let twilight = smoothstep(-0.21, -0.045, sun_altitude);
    let night = 1.0 - smoothstep(-0.19, -0.055, sun_altitude);

    let night_sky = mix(night_horizon.rgb, night_zenith.rgb, gradient) * night_horizon.w;
    let twilight_sky = mix(twilight_horizon.rgb, twilight_zenith.rgb, gradient) * twilight_horizon.w;
    let day_sky = mix(day_horizon.rgb, day_zenith.rgb, gradient) * day_horizon.w;
    var color = mix(night_sky, twilight_sky, twilight);
    color = mix(color, day_sky, daylight);

    var toward_sun = 0.0;
    if length(ray.xz) > 0.0001 && length(sun_dir.xz) > 0.0001 {
        toward_sun = max(0.0, dot(normalize(ray.xz), normalize(sun_dir.xz)));
    }
    let sunset_band = exp(-abs(ray.y) * 8.0) * pow(toward_sun, 7.0)
        * (1.0 - smoothstep(0.02, 0.30, abs(sun_altitude)));
    color += sunset.rgb * sunset_band;
    color += star_field(ray, night);

    let sun_separation = acos(clamp(dot(ray, sun_dir), -1.0, 1.0));
    let sun_halo = exp(-pow(sun_separation / max(sun.z, 0.0001), 1.35)) * sun.w;
    color += vec3(1.0, 0.62, 0.30) * sun_halo;
    if sun_separation < sun.x {
        color += vec3(1.0, 0.91, 0.68) * sun.y;
    }
    color += moon_color(ray);

    // Cloud coverage is composited last: every celestial body and halo is
    // behind it, so increasing rain smoothly hides the entire clear sky.
    let rain = clamp(time_weather_phase.y, 0.0, 1.0);
    let coverage = mix(clouds.x, clouds.y, rain);
    let opacity = smoothstep(1.0 - coverage, 1.12 - coverage, cloud_noise(ray))
        * smoothstep(-0.12, 0.05, ray.y);
    var cloud_brightness = mix(night_horizon.w, twilight_horizon.w, twilight);
    cloud_brightness = mix(cloud_brightness, day_horizon.w, daylight);
    let clouds_rgb = mix(cloud_color.rgb, overcast_color.rgb, rain) * cloud_brightness;
    color = mix(color, clouds_rgb, opacity);
    return vec4(max(color, vec3(0.0)), 1.0);
}
