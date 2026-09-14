#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    decal::clustered::apply_decals,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> surface: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var grass_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var grass_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var soil_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var soil_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var<uniform> grass_color: vec4<f32>;
fn relief_normal(position: vec3<f32>, normal: vec3<f32>, height: f32) -> vec3<f32> {
    let dx = dpdx(position);
    let dy = dpdy(position);
    let x = cross(dy, normal);
    let y = cross(normal, dx);
    let determinant = dot(dx, x);
    let gradient = sign(determinant) * (dpdx(height) * x + dpdy(height) * y);
    return normalize(max(abs(determinant), 0.00000001) * normal - gradient);
}

fn terrain_hash(cell: vec2<i32>) -> f32 {
    var value = bitcast<u32>(cell.x) * 374761393u + bitcast<u32>(cell.y) * 668265263u;
    value = (value ^ (value >> 13u)) * 1274126177u;
    return f32(value ^ (value >> 16u)) / 4294967295.0;
}

fn terrain_noise(position: vec2<f32>) -> f32 {
    let cell = vec2<i32>(floor(position));
    var t = fract(position);
    t = t * t * (vec2(3.0) - t * 2.0);
    let a = mix(terrain_hash(cell), terrain_hash(cell + vec2(1, 0)), t.x);
    let b = mix(terrain_hash(cell + vec2(0, 1)), terrain_hash(cell + vec2(1, 1)), t.x);
    return mix(a, b, t.y);
}

fn terrain_cover(position: vec2<f32>) -> vec3<f32> {
    let coverage_noise = terrain_noise(position / 19.0) * 0.58
        + terrain_noise(position / 6.7 + vec2(31.0)) * 0.29
        + terrain_noise(position / 2.3 + vec2(13.0, 47.0)) * 0.13;
    let soil = smoothstep(0.54, 0.73, coverage_noise);
    let dry = smoothstep(0.28, 0.82, terrain_noise(position / 37.0 + vec2(71.0, 19.0)));
    let shade_noise = terrain_noise(position / 4.8 + vec2(5.0, 29.0)) * 0.65
        + terrain_noise(position / 2.3 + vec2(149.0, 11.0)) * 0.35;
    let region = terrain_noise(position / 19.0 + vec2(61.0, 173.0));
    let shade = (0.52 + smoothstep(0.28, 0.72, shade_noise) * 0.60) * (0.9 + region * 0.2);
    return vec3(soil, dry, shade);
}

// Stochastic triangular tiling gives each region a stable random offset and
// quarter-turn while sharing samples with its neighbours. It removes the
// source image's short repeat without hard seams or view-dependent noise.
struct StochasticFrame {
    position: vec2<f32>,
    gradient_x: vec2<f32>,
    gradient_y: vec2<f32>,
    weights: vec3<f32>,
    id_0: vec2<i32>,
    id_1: vec2<i32>,
    id_2: vec2<i32>,
}

fn stochastic_frame(position: vec2<f32>) -> StochasticFrame {
    let skewed = vec2(position.x - position.y * 0.577350269, position.y * 1.154700538);
    let base = vec2<i32>(floor(skewed));
    let local = fract(skewed);
    let remainder = 1.0 - local.x - local.y;
    var weights: vec3<f32>;
    var id_0: vec2<i32>;
    var id_1: vec2<i32>;
    var id_2: vec2<i32>;
    if remainder > 0.0 {
        weights = vec3(remainder, local.y, local.x);
        id_0 = base;
        id_1 = base + vec2(0, 1);
        id_2 = base + vec2(1, 0);
    } else {
        weights = vec3(-remainder, 1.0 - local.y, 1.0 - local.x);
        id_0 = base + vec2(1, 1);
        id_1 = base + vec2(1, 0);
        id_2 = base + vec2(0, 1);
    }
    // Sharpen the barycentric blend so authored grains remain crisp through
    // most of a region while the three-way transitions stay continuous.
    weights *= weights;
    weights /= dot(weights, vec3(1.0));
    var frame: StochasticFrame;
    frame.position = position;
    frame.gradient_x = dpdx(position);
    frame.gradient_y = dpdy(position);
    frame.weights = weights;
    frame.id_0 = id_0;
    frame.id_1 = id_1;
    frame.id_2 = id_2;
    return frame;
}

fn variant_rotation(id: vec2<i32>, salt: vec2<i32>) -> vec2<f32> {
    let angle = floor(terrain_hash(id + salt) * 4.0) * 1.570796327;
    return vec2(cos(angle), sin(angle));
}

fn apply_rotation(value: vec2<f32>, axis: vec2<f32>) -> vec2<f32> {
    return vec2(axis.x * value.x - axis.y * value.y, axis.y * value.x + axis.x * value.y);
}

fn variant_offset(id: vec2<i32>, salt: vec2<i32>) -> vec2<f32> {
    return vec2(
        terrain_hash(id + salt + vec2(37, 101)),
        terrain_hash(id + salt + vec2(173, 59))
    );
}

fn grass_variant(frame: StochasticFrame, id: vec2<i32>) -> vec3<f32> {
    let salt = vec2(11, 79);
    let axis = variant_rotation(id, salt);
    return textureSampleGrad(
        grass_texture,
        grass_sampler,
        apply_rotation(frame.position, axis) + variant_offset(id, salt),
        apply_rotation(frame.gradient_x, axis),
        apply_rotation(frame.gradient_y, axis)
    ).rgb;
}

fn stochastic_grass(position: vec2<f32>) -> vec3<f32> {
    let frame = stochastic_frame(position);
    return grass_variant(frame, frame.id_0) * frame.weights.x
        + grass_variant(frame, frame.id_1) * frame.weights.y
        + grass_variant(frame, frame.id_2) * frame.weights.z;
}

fn soil_variant(frame: StochasticFrame, id: vec2<i32>) -> vec3<f32> {
    let salt = vec2(193, 41);
    let axis = variant_rotation(id, salt);
    return textureSampleGrad(
        soil_texture,
        soil_sampler,
        apply_rotation(frame.position, axis) + variant_offset(id, salt),
        apply_rotation(frame.gradient_x, axis),
        apply_rotation(frame.gradient_y, axis)
    ).rgb;
}

fn stochastic_soil(position: vec2<f32>) -> vec3<f32> {
    let frame = stochastic_frame(position);
    return soil_variant(frame, frame.id_0) * frame.weights.x
        + soil_variant(frame, frame.id_1) * frame.weights.y
        + soil_variant(frame, frame.id_2) * frame.weights.z;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, is_front);
    // UVs are surface-local metres, so a carried terrain surface keeps its texture in place.
    let uv = in.uv;
    // A slow coordinate warp makes the stochastic regions less geometric
    // without introducing view-dependent sampling or temporal noise.
    let warp = vec2(
        terrain_noise(uv / 11.3 + vec2(17.0, 3.0)),
        terrain_noise(uv / 13.7 + vec2(5.0, 23.0))
    ) * 2.8;
    let grass = stochastic_grass((uv + warp) / surface.x);
    let detail = dot(grass, vec3(0.2126, 0.7152, 0.0722));
    let cover = terrain_cover(uv);
    // Brown is controlled solely by the shared cover field so CPU-generated
    // blades can use the same boundary and never grow in bare patches.
    let bare = smoothstep(0.04, 0.28, cover.r);
    let dry = cover.g;
    // The configured sRGB color reaches this uniform in linear space. Use
    // the meadow texture for value/detail rather than inheriting its yellow
    // hue, so the configured green is the actual visible base color.
    let grass_value = clamp(0.52 + detail * 3.2, 0.45, 1.45);
    let healthy_grass = grass_color.rgb * grass_value;
    let dry_grass = healthy_grass * vec3(1.22, 0.88, 0.55);
    let grass_tint = mix(healthy_grass, dry_grass, dry * 0.55);
    let grass_macro = terrain_noise(uv / 4.6 + vec2(211.0, 43.0)) * 0.70
        + terrain_noise(uv / 2.2 + vec2(17.0, 191.0)) * 0.30;
    let grass_macro_tint = mix(vec3(0.86, 0.92, 0.90), vec3(1.04, 1.12, 1.00), grass_macro);
    // Preserve the authored soil's dense aggregate instead of enlarging
    // procedural value noise into soft blobs. Stochastic tiling breaks the
    // source image's period without washing out its fine detail.
    let soil_warp = vec2(
        terrain_noise(uv / 7.7 + vec2(89.0, 17.0)),
        terrain_noise(uv / 9.1 + vec2(23.0, 157.0))
    ) * 0.24 - vec2(0.12);
    let soil_detail = stochastic_soil((uv + soil_warp) / surface.w);
    let soil_macro = terrain_noise(uv / 4.9 + vec2(181.0, 61.0)) * 0.65
        + terrain_noise(uv / 2.4 + vec2(73.0, 227.0)) * 0.25
        + terrain_noise(uv / 18.0 + vec2(29.0, 103.0)) * 0.10;
    let soil_macro_tint = mix(vec3(0.76, 0.82, 0.88), vec3(1.18, 1.08, 0.92), soil_macro);
    let soil_color = soil_detail * soil_macro_tint;
    let color = mix(grass_tint * grass_macro_tint, soil_color, bare) * cover.b;
    pbr.material.base_color = vec4(color, 1.0);
    let soil_height = dot(soil_detail, vec3(0.2126, 0.7152, 0.0722));
    let height = mix(detail * surface.y, soil_height * surface.z, bare);
    pbr.N = relief_normal(in.world_position.xyz, normalize(in.world_normal), height);
    apply_decals(&pbr);
#ifdef PREPASS_PIPELINE
    return deferred_output(in, pbr);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr);
    out.color = main_pass_post_lighting_processing(pbr, out.color);
    return out;
#endif
}
