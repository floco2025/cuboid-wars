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
    let shade = 0.9 + terrain_noise(position / 9.0 + vec2(5.0, 29.0)) * 0.22;
    return vec3(soil, dry, shade);
}

fn rotated(position: vec2<f32>) -> vec2<f32> {
    return vec2(-position.y, position.x);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, is_front);
    // UVs are surface-local metres, so a carried terrain surface keeps its texture in place.
    let uv = in.uv;
    // Coordinate warping plus incommensurate rotated samples hides the tile
    // period without introducing view-dependent sampling or temporal noise.
    let warp = vec2(
        terrain_noise(uv / 11.3 + vec2(17.0, 3.0)),
        terrain_noise(uv / 13.7 + vec2(5.0, 23.0))
    ) * 2.8;
    let grass_a = textureSample(grass_texture, grass_sampler, (uv + warp) / surface.x).rgb;
    let grass_b = textureSample(
        grass_texture,
        grass_sampler,
        rotated(uv - warp * 0.4) / (surface.x * 1.371) + vec2(0.31, 0.67)
    ).rgb;
    let grass_c = textureSample(
        grass_texture,
        grass_sampler,
        (uv * vec2(-0.63, 0.81) + rotated(uv) * vec2(0.37, 0.19)) / (surface.x * 2.173)
            + vec2(0.73, 0.11)
    ).rgb;
    let grass_mix = terrain_noise(uv / 4.9 + vec2(41.0, 83.0));
    let grass = mix(mix(grass_a, grass_b, 0.38), grass_c, 0.16 + grass_mix * 0.16);
    let detail = dot(grass, vec3(0.2126, 0.7152, 0.0722));
    let cover = terrain_cover(uv);
    // Brown is controlled solely by the shared cover field so CPU-generated
    // blades can use the same boundary and never grow in bare patches.
    let bare = smoothstep(0.04, 0.28, cover.r);
    let dry = cover.g;
    let grass_tint = mix(vec3(1.12, 1.22, 1.04), vec3(1.48, 1.24, 0.82), dry);
    // Brown patches are generated from world-stable, non-periodic noise. The
    // retired exterior ground texture is not sampled by this material.
    let soil_coarse = terrain_noise(uv / 1.73 + vec2(113.0, 7.0));
    let soil_fine = terrain_noise(uv * 2.91 + vec2(17.0, 131.0));
    let soil_grain = terrain_noise(rotated(uv) * 8.37 + vec2(59.0, 23.0));
    let soil_value = soil_coarse * 0.52 + soil_fine * 0.33 + soil_grain * 0.15;
    let damp = terrain_noise(uv / 8.1 + vec2(97.0, 43.0));
    let soil_dark = vec3(0.095, 0.061, 0.032);
    let soil_light = vec3(0.245, 0.155, 0.075);
    let soil_color = mix(soil_dark, soil_light, soil_value) * mix(0.82, 1.1, damp);
    let color = mix(grass * grass_tint, soil_color, bare) * cover.b;
    pbr.material.base_color = vec4(color, 1.0);
    let height = mix(detail, soil_value, bare) * surface.y;
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
