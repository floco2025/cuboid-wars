// The translucent field pane: standard PBR shading of the kind material with
// a hex pattern in carrier-frame metres (`uv`), a Fresnel glow toward grazing
// angles, and a band that brightens toward the frame (`uv_b.x`, 0 at the
// rails and 1 inside). Main pass only: a Blend material has no prepass or
// shadow pass.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
}

// x = hex cell width across flats (m), y = line width (m), z = line glow, w = Fresnel power.
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> field_pattern: vec4<f32>;
// x = head-on opacity as a fraction of the pane alpha, y = edge glow.
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<uniform> field_shape: vec4<f32>;

// Pointy-top hexes one unit wide across the flats repeat on this lattice.
const HEX_LATTICE: vec2<f32> = vec2<f32>(1.0, 1.7320508);

// Distance (m) from `p` to the nearest edge of the hex grid whose cells are `cell` wide.
fn hex_edge_distance(p: vec2<f32>, cell: f32) -> f32 {
    let uv = p / cell;
    let half = HEX_LATTICE * 0.5;
    let a = fract(uv / HEX_LATTICE) * HEX_LATTICE - half;
    let b = fract((uv - half) / HEX_LATTICE) * HEX_LATTICE - half;
    var local = a;
    if dot(b, b) < dot(a, a) {
        local = b;
    }
    let q = abs(local);
    let center_distance = max(dot(q, normalize(HEX_LATTICE)), q.x);
    return (0.5 - center_distance) * cell;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let edge_m = hex_edge_distance(in.uv, field_pattern.x);
    let line = 1.0 - smoothstep(field_pattern.y, field_pattern.y + fwidth(edge_m), edge_m);
    let fresnel = pow(1.0 - saturate(dot(pbr_input.N, pbr_input.V)), field_pattern.w);
    let rim = max(fresnel, 1.0 - in.uv_b.x);
    let glow = max(line, rim);
    var color = pbr_input.material.base_color;
    color.a = color.a * mix(field_shape.x, 1.0, glow);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, color);
    let emissive = pbr_input.material.emissive;
    pbr_input.material.emissive = vec4<f32>(
        emissive.rgb * (1.0 + line * field_pattern.z + rim * field_shape.y),
        emissive.a,
    );
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
