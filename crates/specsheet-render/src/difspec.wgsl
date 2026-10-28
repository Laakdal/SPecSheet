// The dif.spec family, written from docs/app/shader-specs/dif-spec.md §5
// (D13): diffuse, the base alpha as specular mask, and the add.env
// reflection, lit by the game's forward model (game_lighting.wgsl).

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
}
#import specsheet::game_lighting::{
    GameLight, eye_vector, sun_light, reflection_visibility, reflection_lookup,
    linear_fresnel, to_output,
}

// Mirrors `DifSpecUniform` in difspec.rs; the symbols are the spec's.
struct DifSpecUniform {
    specular: vec4<f32>,    // Ks, p
    environment: vec4<f32>, // Ke, ka
    fresnel: vec4<f32>,     // fb, fs, ms, mb
    reflection: vec4<f32>,  // g
    flags: u32,
}

const WEIGHT: u32 = 1u;
const ENV: u32 = 2u;
const NOFRESNEL: u32 = 4u;
const REFLECTION_TEXTURE: u32 = 8u;

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> material: DifSpecUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<storage, read> light: GameLight;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var reflection_texture: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var reflection_sampler: sampler;

fn has(flag: u32) -> bool {
    return (material.flags & flag) != 0u;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    let pbr_input = pbr_input_from_standard_material(in, is_front);
    // B.rgb · Kd · c2 (the vertex colour is uploaded doubled), and B.a · c.a.
    let base = pbr_input.material.base_color;
    // The `a` flavour's discard and the output alpha of `over`.
    let output_alpha = alpha_discard(pbr_input.material, base).a;
    let mask = base.a;
    let n = pbr_input.N;
    let p = in.world_position.xyz - view.world_position;
    let e = eye_vector(p);
    let weight = has(WEIGHT);
#ifdef VERTEX_COLORS
    let c2 = in.color.rgb;
#else
    let c2 = vec3<f32>(1.0);
#endif

    // Surface values (§5.3).
    let ks = material.specular.rgb;
    let remapped = saturate(material.fresnel.z * mask + material.fresnel.w) * ks;
    let specular = select(remapped, mask * ks * c2, weight);

    // Sun and ambient (§5.4) and the combination (§5.6).
    let sun = sun_light(light, n, e, material.specular.w, weight);
    var lit = (sun.diffuse + light.additional_ambient.rgb * material.environment.w) * base.rgb
        + sun.specular * specular;

    // The reflection (§5.5).
    let cube = textureSample(reflection_texture, reflection_sampler, reflection_lookup(p, n)).rgb;
    let fresnel = select(linear_fresnel(material.fresnel.x, material.fresnel.y, dot(n, e)), 1.0, has(NOFRESNEL));
    let tint = select(vec3<f32>(1.0), c2, weight);
    let strength = material.reflection.x * mask * fresnel;
    let reflection = cube * material.environment.rgb * tint * strength * reflection_visibility(light);
    lit += select(vec3<f32>(0.0), reflection, has(ENV) && has(REFLECTION_TEXTURE));

    var out: FragmentOutput;
    out.color = vec4<f32>(to_output(light, lit), output_alpha);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
