// Standard PBR shading with one world-space clip plane: fragments behind it
// are discarded in the main pass, the depth and deferred prepasses, and the
// shadow passes, so a body drawn on one side of a portal plane never shows
// through the surface behind that plane.

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_prepass_functions::{prepass_alpha_discard, calculate_motion_vector},
}
#ifdef DEFERRED_PREPASS
#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    pbr_deferred_functions::deferred_output,
}
#endif
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
}
#endif

// xyz = plane normal, w = -normal · plane point; kept where the signed distance is not negative.
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> portal_clip_plane: vec4<f32>;

fn discard_behind_plane(world_position: vec4<f32>) {
    if dot(world_position.xyz, portal_clip_plane.xyz) + portal_clip_plane.w < 0.0 {
        discard;
    }
}

#ifdef PREPASS_PIPELINE
#ifdef PREPASS_FRAGMENT
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    discard_behind_plane(in.world_position);
    var out: FragmentOutput;
#ifdef DEFERRED_PREPASS
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    out = deferred_output(in, pbr_input);
#else
    prepass_alpha_discard(in);
#ifdef NORMAL_PREPASS
    out.normal = vec4(in.world_normal * 0.5 + vec3(0.5), 1.0);
#endif
#ifdef MOTION_VECTOR_PREPASS
    out.motion_vector = calculate_motion_vector(in.world_position, in.previous_world_position);
#endif
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.frag_depth = in.unclipped_depth;
#endif
#endif
    return out;
}
#else
@fragment
fn fragment(in: VertexOutput) {
    discard_behind_plane(in.world_position);
    prepass_alpha_discard(in);
}
#endif
#else
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    discard_behind_plane(in.world_position);
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
#endif
