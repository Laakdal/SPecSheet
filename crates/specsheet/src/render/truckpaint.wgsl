// The truck paint surface and its lighting, written from
// docs/app/shader-specs/truckpaint.md and truckpaint-lighting.md (D13): the
// paint colour, masks, liveries, flip colour and flakes, lit by the game's
// forward model (game_lighting.wgsl).

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::main_pass_post_lighting_processing,
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
}
#import specsheet::game_lighting::{
    GameLight, eye_vector, sun_light, reflection_visibility, reflection_lookup,
    schlick_fresnel, to_output,
}

// Mirrors `PaintUniform` in truckpaint.rs; the symbols are the spec's.
struct PaintUniform {
    base: vec4<f32>,          // P
    mask_r: vec4<f32>,        // Q_r
    mask_g: vec4<f32>,        // Q_g
    mask_b: vec4<f32>,        // Q_b
    mask_constant: vec4<f32>, // M when no mask texture is bound
    flip: vec4<f32>,          // K_flip, φ
    flake: vec4<f32>,         // K_flake, ρ
    flake_setup: vec4<f32>,   // s, p_f, k, r
    specular: vec4<f32>,      // S_mat, p_mat
    shading: vec4<f32>,       // F0, λ, a_mat, μ
    flags: u32,
}

const COLORMASK: u32 = 1u;
const AIRBRUSH: u32 = 2u;
const FLIPFLAKE: u32 = 4u;
const MASK_TEXTURE: u32 = 16u;
const FLAKE_TEXTURE: u32 = 32u;
const REFLECTION_TEXTURE: u32 = 64u;

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> paint: PaintUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var mask_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var mask_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var flake_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var flake_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var<storage, read> light: GameLight;
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var reflection_texture: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(107) var reflection_sampler: sampler;

fn has(flag: u32) -> bool {
    return (paint.flags & flag) != 0u;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    let pbr_input = pbr_input_from_standard_material(in, is_front);
    // B.rgb × .mat diffuse × v₂ (the vertex colour is uploaded doubled), and
    // B.a × vertex alpha.
    let surface = pbr_input.material.base_color;
    let n = pbr_input.N;
    let p = in.world_position.xyz - view.world_position;
    let e = eye_vector(p);
#ifdef VERTEX_COLORS
    let v2 = in.color.rgb;
#else
    let v2 = vec3<f32>(1.0);
#endif

    // The paint UV: the mesh carries the set this paint samples as UV_1.
#ifdef VERTEX_UVS_B
    let uv_paint = in.uv_b;
#else ifdef VERTEX_UVS_A
    let uv_paint = in.uv;
#else
    let uv_paint = vec2<f32>(0.0);
#endif

    let sampled_mask = textureSample(mask_texture, mask_sampler, uv_paint);
    let mask = select(paint.mask_constant, sampled_mask, has(MASK_TEXTURE));

    // Paint colour.
    var color = paint.base.rgb;
    if has(FLIPFLAKE) {
        let facing = saturate(dot(n, e));
        let averted = 1.0 - facing;
        let phi = paint.flip.w;
        let toward = mix(facing, facing * facing, phi);
        let away = mix(averted, averted * averted, phi);
        color = toward * paint.base.rgb + away * paint.flip.rgb;
    }
    if has(COLORMASK) {
        color = mix(color, paint.mask_b.rgb, mask.b);
        color = mix(color, paint.mask_g.rgb, mask.g);
        color = mix(color, paint.mask_r.rgb, mask.r);
    }
    if has(AIRBRUSH) {
        color = mix(color, mask.rgb, mask.a);
    }

    // Flakes change only the highlight's colour and sharpness; the flake
    // light is taken to be the sun (materials.md).
    let scale = paint.flake_setup.x;
    let flake_uv = vec2<f32>(uv_paint.x * scale, uv_paint.y * scale * paint.flake_setup.w);
    let noise = textureSample(flake_texture, flake_sampler, flake_uv);
    var spec_color = paint.specular.rgb;
    var power = paint.specular.w;
    if has(FLIPFLAKE) && has(FLAKE_TEXTURE) {
        let coverage = 1.0 - paint.flake.w * noise.a;
        let strength = saturate(paint.shading.y);
        let l = light.to_sun.xyz;
        let h = select(0.0, saturate(dot(n, normalize(e + l))), dot(n, l) >= 0.0);
        let k = paint.flake_setup.z;
        let tau = clamp(exp(0.996 * k) - exp(k * h), 0.0, 1.0);
        spec_color = mix(spec_color, noise.rgb * paint.flake.rgb, tau * strength);
        power = mix(power, paint.flake_setup.y, coverage * strength);
    }

    // Surface values (lighting §4), with (a_w, b_w) = (1, 0).
    let m_spec = select(1.0, mask.a, has(COLORMASK));
    let albedo = surface.rgb * color;
    let specular = saturate(surface.a) * m_spec * spec_color * v2;
    let a_mat = paint.shading.z;

    // Forward lighting (lighting §5).
    let sun = sun_light(light, n, e, power, false);
    var lit = (sun.diffuse + light.additional_ambient.rgb * a_mat) * albedo + sun.specular * specular;

    // The environment reflection with Schlick's fresnel.
    let cube = textureSample(reflection_texture, reflection_sampler, reflection_lookup(p, n)).rgb;
    let fresnel = schlick_fresnel(paint.shading.x, dot(n, e));
    let k_env = paint.shading.w * surface.a * m_spec * fresnel;
    let reflection = select(vec3<f32>(0.0), cube * v2 * k_env, has(REFLECTION_TEXTURE));
    lit += reflection_visibility(light) * reflection;

    var out: FragmentOutput;
    out.color = vec4<f32>(to_output(light, lit), 1.0);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
