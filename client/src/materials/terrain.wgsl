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
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var cover_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var cover_sampler: sampler;

fn relief_normal(position: vec3<f32>, normal: vec3<f32>, height: f32) -> vec3<f32> {
    let dx = dpdx(position);
    let dy = dpdy(position);
    let x = cross(dy, normal);
    let y = cross(normal, dx);
    let determinant = dot(dx, x);
    let gradient = sign(determinant) * (dpdx(height) * x + dpdy(height) * y);
    return normalize(max(abs(determinant), 0.00000001) * normal - gradient);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, is_front);
    // UVs are surface-local metres, so a carried terrain surface keeps its texture in place.
    let uv = in.uv;
    let grass_a = textureSample(grass_texture, grass_sampler, uv / surface.x).rgb;
    let grass_b = textureSample(grass_texture, grass_sampler, vec2(-uv.y, uv.x) / (surface.x * 1.37) + vec2(0.31, 0.67)).rgb;
    let grass = mix(grass_a, grass_b, 0.35);
    let soil = textureSample(soil_texture, soil_sampler, uv / surface.y).rgb;
    let detail = dot(grass, vec3(0.2126, 0.7152, 0.0722));
    let cover = textureSample(cover_texture, cover_sampler, uv / surface.w).rgb;
    let bare = smoothstep(0.18, 0.82, cover.r + (detail - 0.12) * 1.4);
    let dry = cover.g;
    let grass_tint = mix(vec3(0.82, 0.98, 0.88), vec3(1.55, 1.21, 0.85), dry);
    let soil_color = soil * vec3(1.3, 1.14, 0.98) + vec3(0.055, 0.042, 0.025);
    let color = mix(grass * grass_tint, soil_color, bare) * cover.b * 2.0;
    pbr.material.base_color = vec4(color, 1.0);
    let height = mix(detail, dot(soil, vec3(0.3333)), bare) * surface.z;
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
