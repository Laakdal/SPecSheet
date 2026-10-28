// The truck paint surface, written from docs/app/shader-specs/truckpaint.md
// (D13). The spec's paint colour, masks, liveries, flip colour and flakes
// feed Bevy's PBR lighting; materials.md describes the mapping.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::lights,
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
    shading: vec4<f32>,       // F0, λ
    flags: u32,
}

const COLORMASK: u32 = 1u;
const AIRBRUSH: u32 = 2u;
const FLIPFLAKE: u32 = 4u;
const MASK_TEXTURE: u32 = 16u;
const FLAKE_TEXTURE: u32 = 32u;

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> paint: PaintUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var mask_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var mask_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var flake_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var flake_sampler: sampler;

fn has(flag: u32) -> bool {
    return (paint.flags & flag) != 0u;
}

// Blinn-Phong power to Bevy's perceptual roughness, as assets.rs maps it.
fn roughness_from_power(power: f32) -> f32 {
    return clamp(sqrt(sqrt(2.0 / (max(power, 0.0) + 2.0))), 0.089, 1.0);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    // Base texture × .mat diffuse × doubled vertex colour (uploaded doubled).
    let surface = pbr_input.material.base_color;
    let n = pbr_input.N;
    let v = pbr_input.V;

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
        let facing = saturate(dot(n, v));
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

    // Flakes change only the highlight's colour and sharpness.
    let scale = paint.flake_setup.x;
    let flake_uv = vec2<f32>(uv_paint.x * scale, uv_paint.y * scale * paint.flake_setup.w);
    let noise = textureSample(flake_texture, flake_sampler, flake_uv);
    var spec_color = paint.specular.rgb;
    var power = paint.specular.w;
    if has(FLIPFLAKE) && has(FLAKE_TEXTURE) && lights.n_directional_lights > 0u {
        let coverage = 1.0 - paint.flake.w * noise.a;
        let strength = saturate(paint.shading.y);
        // The flake light is taken to be the sun (materials.md).
        let l = lights.directional_lights[0].direction_to_light;
        let h_vec = normalize(v + l);
        let h = select(0.0, saturate(dot(n, h_vec)), dot(n, l) >= 0.0);
        let k = paint.flake_setup.z;
        let tau = clamp(exp(0.996 * k) - exp(k * h), 0.0, 1.0);
        spec_color = mix(spec_color, noise.rgb * paint.flake.rgb, tau * strength);
        power = mix(power, paint.flake_setup.y, coverage * strength);
    }

    // To PBR: the colour mask's alpha and the base alpha scale the specular
    // and the reflection; the specular colour's ratio to the .mat's tints F0.
    let m_spec = select(1.0, mask.a, has(COLORMASK));
    let peak = max(paint.specular.r, max(paint.specular.g, paint.specular.b));
    let tint = clamp(spec_color / max(peak, 0.001), vec3<f32>(0.0), vec3<f32>(4.0));
    let f0 = paint.shading.x * tint * saturate(surface.a) * m_spec;
    pbr_input.material.reflectance = sqrt(f0 / 0.16);
    pbr_input.material.metallic = 0.0;
    pbr_input.material.perceptual_roughness = roughness_from_power(power);
    pbr_input.material.base_color = vec4<f32>(surface.rgb * color, surface.a);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
