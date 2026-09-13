#import bevy_pbr::{
    forward_io::VertexOutput,
    mesh_view_bindings::view,
    utils::coords_to_viewport_uv,
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
// angular radius, luminance, earthshine
@group(#{MATERIAL_BIND_GROUP}) @binding(12) var<uniform> moon: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(13) var<uniform> moon_halo: vec4<f32>;
// seed, density, min luminance, max luminance
@group(#{MATERIAL_BIND_GROUP}) @binding(14) var<uniform> stars: vec4<f32>;
// bright fraction, twinkle
@group(#{MATERIAL_BIND_GROUP}) @binding(15) var<uniform> star_detail: vec4<f32>;
// clear coverage, overcast coverage, scale, angular wind speed (radians/sec)
@group(#{MATERIAL_BIND_GROUP}) @binding(16) var<uniform> clouds: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(17) var<uniform> cloud_color: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(18) var<uniform> overcast_color: vec4<f32>;

const PI: f32 = 3.14159265359;
fn hash13(p: vec3<f32>) -> f32 {
    return fract(sin(dot(p, vec3(127.1, 311.7, 74.7))) * 43758.5453123);
}

fn hash31(p: vec3<f32>) -> f32 {
    var p3 = fract(p * vec3(0.1031, 0.1030, 0.0973));
    p3 += dot(p3, p3.yzx + vec3(33.33));
    return fract((p3.x + p3.y) * p3.z);
}

fn cloud_noise_3d(p: vec3<f32>) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    return mix(
        mix(
            mix(hash31(cell), hash31(cell + vec3(1.0, 0.0, 0.0)), u.x),
            mix(hash31(cell + vec3(0.0, 1.0, 0.0)), hash31(cell + vec3(1.0, 1.0, 0.0)), u.x),
            u.y,
        ),
        mix(
            mix(hash31(cell + vec3(0.0, 0.0, 1.0)), hash31(cell + vec3(1.0, 0.0, 1.0)), u.x),
            mix(hash31(cell + vec3(0.0, 1.0, 1.0)), hash31(cell + vec3(1.0, 1.0, 1.0)), u.x),
            u.y,
        ),
        u.z,
    );
}

fn rotate_scale_3d(p: vec3<f32>) -> vec3<f32> {
    return vec3(
        p.y * 1.23 + p.z * 1.57,
        p.z * 1.11 - p.x * 1.65,
        p.x * 1.39 - p.y * 1.43,
    ) + vec3(7.13, -5.71, 3.91);
}

fn cloud_fbm_3(p_initial: vec3<f32>) -> f32 {
    var p = p_initial;
    var amplitude = 0.5714;
    var total = 0.0;
    for (var octave = 0; octave < 3; octave += 1) {
        total += cloud_noise_3d(p) * amplitude;
        p = rotate_scale_3d(p);
        amplitude *= 0.5;
    }
    return total / 0.9999;
}

// This is the distant upper-shell layer from the pre-cumulus experiment.
// Its directional stretch gives the background its long, soft formations.
fn high_cloud_density(position: vec3<f32>) -> f32 {
    let stretched = vec3(
        position.x * 0.78 + position.z * 0.16,
        position.y * 1.12,
        position.z * 1.62 - position.x * 0.09,
    );
    let band = cloud_fbm_3(stretched);
    let wisps = 1.0 - abs(band * 2.0 - 1.0);
    return wisps * 0.58 + band * 0.42;
}

fn cloud_shell_position(direction: vec3<f32>, height: f32) -> vec3<f32> {
    let planet_radius = 5.0;
    let b = planet_radius * direction.y;
    let shell_term = 2.0 * planet_radius * height + height * height;
    let distance = -b + sqrt(max(0.0, b * b + shell_term));
    return (vec3(0.0, planet_radius, 0.0) + direction * distance) * clouds.z;
}

struct CloudSample {
    opacity: f32,
    shape: f32,
};

fn sample_background_clouds(direction: vec3<f32>, coverage: f32, rain: f32) -> CloudSample {
    let wind = vec3(time_weather_phase.x * clouds.w, 0.0, time_weather_phase.x * clouds.w * 0.31);
    let position = cloud_shell_position(direction, 2.35) * 0.78
        + wind * 1.65
        + vec3(31.0, -17.0, 11.0);
    let density = high_cloud_density(position);

    let horizon = 1.0 - smoothstep(0.015, 0.34, direction.y);
    let threshold = 0.75 - coverage * 0.45 - horizon * (0.035 + coverage * 0.035);
    let cloud_threshold = threshold + mix(0.08, 0.15, rain);
    var opacity = smoothstep(cloud_threshold - 0.035, cloud_threshold + 0.085, density)
        * mix(0.20, 0.96, rain);
    // The same distant sheet closes into an unbroken deck as overcast
    // arrives. This reaches full cover before the separate precipitation
    // envelope starts, without bringing back a foreground layer.
    opacity = mix(opacity, 1.0, smoothstep(0.55, 0.95, rain));
    opacity *= smoothstep(-0.055, 0.025, direction.y);
    return CloudSample(opacity, density);
}

// Reconstructing the ray from the fragment coordinate avoids interpolating
// directions across the sky mesh. Procedural detail therefore stays smooth
// across triangle boundaries and remains stable at large world positions.
fn camera_ray(position: vec2<f32>) -> vec3<f32> {
    let view_position = view.view_from_clip * vec4(
        coords_to_viewport_uv(position, view.viewport) * vec2(2.0, -2.0) + vec2(-1.0, 1.0),
        1.0,
        1.0,
    );
    let view_direction = view_position.xyz / view_position.w;
    return normalize((view.world_from_view * vec4(view_direction, 0.0)).xyz);
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
    let earthshine = moon.z * (0.45 + 0.55 * max(0.0, -surface_normal.y));
    let surface_light = max(earthshine, direct);
    let limb = smoothstep(0.0, 0.12, z);
    result += vec3(0.86, 0.91, 1.0) * moon.y * surface_light * limb;
    return result;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let ray = camera_ray(in.position.xy);
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

    // Only the distant upper layer remains. The lower layer that produced a
    // few large grey foreground blobs has deliberately been removed.
    let rain = clamp(time_weather_phase.y, 0.0, 1.0);
    let coverage = mix(clouds.x, clouds.y, rain);
    let cloud = sample_background_clouds(ray, coverage, rain);
    let night_cloud = mix(night_horizon.rgb, night_zenith.rgb, 0.45) * night_horizon.w * 1.75;
    let twilight_cloud = mix(twilight_horizon.rgb, twilight_zenith.rgb, 0.25)
        * twilight_horizon.w * 1.15;
    let day_shadow = mix(overcast_color.rgb, vec3(0.39, 0.43, 0.48), 1.0 - rain)
        * day_horizon.w;
    let sun_height = smoothstep(-0.08, 0.42, sun_dir.y);
    let cloud_relief = smoothstep(0.30, 0.78, cloud.shape);
    let day_lit = cloud_color.rgb * day_horizon.w * mix(0.62, 1.08, sun_height);
    let fair_weather_cloud = mix(day_shadow, day_lit, cloud_relief);
    let storm_cloud = day_shadow * mix(0.72, 1.05, cloud_relief);
    var cloud_rgb = mix(night_cloud, twilight_cloud, twilight);
    cloud_rgb = mix(cloud_rgb, mix(fair_weather_cloud, storm_cloud, rain), daylight);
    let sun_facing = pow(max(0.0, dot(ray, sun_dir)), 10.0);
    let silver_lining = sun_facing * (1.0 - cloud.opacity) * cloud.opacity * daylight * 2.2;
    cloud_rgb += vec3(1.0, 0.72, 0.44) * silver_lining;
    cloud_rgb += sunset.rgb * sunset_band * cloud.opacity * 0.32;
    color = mix(color, cloud_rgb, cloud.opacity);
    return vec4(max(color, vec3(0.0)), 1.0);
}
