// The dif.spec family, written from docs/app/shader-specs/dif-spec.md §5
// (D13): diffuse, the base alpha as specular mask, the tangent-space normal
// maps and the add.env reflection, lit by the game's forward model
// (game_lighting.wgsl). The mult base and dif.spec.decal.mult follow
// docs/app/shader-specs/dif-spec-mult.md.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
}
#import specsheet::game_lighting::{
    GameLight, eye_vector, sun_light, sun_visibility, reflection_visibility, reflection_lookup,
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
const TSNMAP: u32 = 16u;
const TSNMAPUV: u32 = 32u;
const TSNMAPUV2: u32 = 64u;
const NORMAL_TEXTURE: u32 = 128u;
const DETAIL_TEXTURE: u32 = 256u;
const MULT: u32 = 512u;
const MULT_TEXTURE: u32 = 1024u;
const MULT_ON_UV0: u32 = 2048u;
const NORMAL_ON_UV0: u32 = 4096u;

// UV0, and UV_1: the set the piece routes to UV slot 1 on mult pieces
// (texture_mult), else the "UV Normal" stream (UV slot 6) on tsnmapuv
// pieces. MULT_ON_UV0 and NORMAL_ON_UV0 say which of the two is the UV0 set
// instead (difspec.rs, route_uvs).
#ifdef VERTEX_UVS_A
const HAS_UV0: bool = true;
#else
const HAS_UV0: bool = false;
#endif
#ifdef VERTEX_UVS_B
const HAS_UV_B: bool = true;
#else
const HAS_UV_B: bool = false;
#endif

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> material: DifSpecUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<storage, read> light: GameLight;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var reflection_texture: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var reflection_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var normal_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var normal_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var detail_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(107) var detail_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(108) var mult_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(109) var mult_sampler: sampler;

fn has(flag: u32) -> bool {
    return (material.flags & flag) != 0u;
}

// A tangent-space normal from a normal-map texel (§4): x and y from r and
// g, so that a byte of 128 is 0; b and a are ignored.
fn decode_normal(texel: vec4<f32>) -> vec3<f32> {
    let xy = texel.rg * (255.0 / 127.0) - vec2<f32>(128.0 / 127.0);
    return vec3<f32>(xy, sqrt(saturate(1.0 - dot(xy, xy))));
}

// fn, the normal map's factor on the albedo (§5.2).
fn diffuse_factor(z: f32) -> f32 {
    return saturate(0.399902 * z + 0.600098);
}

// v / |v|, or 0 for a zero vector.
fn safe_unit(v: vec3<f32>) -> vec3<f32> {
    return v * inverseSqrt(max(dot(v, v), 1e-20));
}

// The shading normal N and the albedo factor fn (§5.2).
struct SurfaceNormal {
    n: vec3<f32>,
    albedo: f32,
}

// The two UV attributes, 0 where the mesh lacks one.
struct Uvs {
    uv0: vec2<f32>,
    uv_b: vec2<f32>,
}

fn mesh_uvs(in: VertexOutput) -> Uvs {
    var out: Uvs;
    out.uv0 = vec2<f32>(0.0);
    out.uv_b = vec2<f32>(0.0);
#ifdef VERTEX_UVS_A
    out.uv0 = in.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = in.uv_b;
#endif
    return out;
}

// §5.2.1 without a normal map (or without what it needs): the vertex normal
// as the standard material prepares it, and fn = 1.
fn surface_normal(in: VertexOutput, uvs: Uvs, n_vertex: vec3<f32>, nv: vec3<f32>, p: vec3<f32>) -> SurfaceNormal {
    var out: SurfaceNormal;
    out.n = n_vertex;
    out.albedo = 1.0;
#ifdef VERTEX_TANGENTS
    let uv0 = uvs.uv0;
    // The "UV Normal" stream: UV_1, or UV0 where the mult layer holds UV_1
    // and the stream is the UV0 set.
    let normal_on_uv0 = has(NORMAL_ON_UV0);
    let uv_normal = select(uvs.uv_b, uv0, normal_on_uv0);
    let has_uv_normal = select(HAS_UV_B, HAS_UV0, normal_on_uv0);
    // The interpolated vertex frame (§3.2): Tv, and Bv = (Nv × Tv)·w.
    let tv = in.world_tangent.xyz;
    let bv = cross(nv, tv) * in.world_tangent.w;
    let on_uv_normal = has(TSNMAPUV) || has(TSNMAPUV2);
    let nm = decode_normal(textureSample(normal_texture, normal_sampler, select(uv0, uv_normal, on_uv_normal)));

    // §5.2.2, tsnmap and tsnmapuv: one map in the vertex frame.
    let n_one_map = normalize(nm.x * tv + nm.y * bv + nm.z * nv);

    // §5.2.3, tsnmapuv2: the main map framed by the screen-space gradients
    // of (u, 1 − v) on the UV Normal stream, moved into the vertex frame
    // and added to the detail map on UV0.
    let nd = decode_normal(textureSample(detail_texture, detail_sampler, uv0));
    let uv_up = vec2<f32>(uv_normal.x, 1.0 - uv_normal.y);
    let dp1 = dpdx(p);
    let dp2 = dpdy(p);
    let duv1 = dpdx(uv_up);
    let duv2 = dpdy(uv_up);
    let dp2_perp = cross(dp2, nv);
    let dp1_perp = cross(nv, dp1);
    // The gradients are these sums over Nv·(dp1 × dp2); its sign keeps t
    // and b pointing towards increasing u' and v' whichever way screen y
    // runs.
    let side = select(-1.0, 1.0, dot(nv, cross(dp1, dp2)) >= 0.0);
    let t = safe_unit(side * (dp2_perp * duv1.x + dp1_perp * duv2.x));
    let b = safe_unit(side * (dp2_perp * duv1.y + dp1_perp * duv2.y));
    let nm_frame = nm.x * t + nm.y * b + nm.z * nv;
    let r = vec3<f32>(dot(tv, nm_frame), dot(bv, nm_frame), dot(nv, nm_frame));
    let k = saturate((1.0 - r.z) / 0.01);
    let combined = normalize(vec3<f32>(nd.x + k * r.x, nd.y + k * r.y, nd.z));
    let n_two_maps = normalize(combined.x * tv + combined.y * bv + combined.z * nv);

    let one_map = (has(TSNMAP) && HAS_UV0) || (has(TSNMAPUV) && has_uv_normal);
    let two_maps = has(TSNMAPUV2) && has(DETAIL_TEXTURE) && HAS_UV0 && has_uv_normal;
    let mapped = has(NORMAL_TEXTURE) && (one_map || two_maps);
    let n = select(n_one_map, n_two_maps, two_maps);
    let albedo = select(diffuse_factor(nm.z), diffuse_factor(abs(combined.z)), two_maps);
    out.n = select(out.n, n, mapped);
    out.albedo = select(out.albedo, albedo, mapped);
#endif
    return out;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    let pbr_input = pbr_input_from_standard_material(in, is_front);
    // B.rgb · Kd · c2 (the vertex colour is uploaded doubled), and B.a · c.a.
    let base = pbr_input.material.base_color;
    // The `a` flavour's discard and the output alpha of `over`.
    let output_alpha = alpha_discard(pbr_input.material, base).a;
    let mask = base.a;
    let p = in.world_position.xyz - view.world_position;
    let uvs = mesh_uvs(in);
    let normal = surface_normal(in, uvs, pbr_input.N, pbr_input.world_normal, p);
    let n = normal.n;
    let e = eye_vector(p);
    let weight = has(WEIGHT);
#ifdef VERTEX_COLORS
    let c2 = in.color.rgb;
#else
    let c2 = vec3<f32>(1.0);
#endif

    // The mult layer M (dif-spec-mult §3) on the set routed to UV slot 1,
    // decoded by its format, as sampled; white without the layer.
    let mult_on_uv0 = has(MULT_ON_UV0);
    let mult_sample = textureSample(mult_texture, mult_sampler, select(uvs.uv_b, uvs.uv0, mult_on_uv0));
    let is_mult = has(MULT);
    let mult_bound = is_mult && has(MULT_TEXTURE) && select(HAS_UV_B, HAS_UV0, mult_on_uv0);
    let m = select(vec4<f32>(1.0), mult_sample, mult_bound);

    // Surface values (§5.3).
    let ks = material.specular.rgb;
    let remapped = saturate(material.fresnel.z * mask + material.fresnel.w) * ks;
    // The mult base: B.a·M.a·Ks, no remap, no vertex colour (dif-spec-mult §3).
    let specular = select(select(remapped, mask * ks * c2, weight), mask * m.a * ks, is_mult);

    // Sun and ambient (§5.4) and the combination (§5.6).
    let shadow = sun_visibility(in.world_position, normalize(in.world_normal), in.position.xy);
    let sun = sun_light(light, n, e, material.specular.w, weight, shadow);
    // fn tints the albedo only (§5.2); M multiplies it (dif-spec-mult §3).
    let albedo = base.rgb * m.rgb * normal.albedo;
    var lit = (sun.diffuse + light.additional_ambient.rgb * material.environment.w) * albedo
        + sun.specular * specular;

    // The reflection (§5.5).
    let cube = textureSample(reflection_texture, reflection_sampler, reflection_lookup(p, n)).rgb;
    let fresnel = select(linear_fresnel(material.fresnel.x, material.fresnel.y, dot(n, e)), 1.0, has(NOFRESNEL));
    let tint = select(vec3<f32>(1.0), c2, weight);
    // M.a masks the reflection too (dif-spec-mult §4.3).
    let strength = material.reflection.x * mask * m.a * fresnel;
    let reflection = cube * material.environment.rgb * tint * strength * reflection_visibility(light);
    lit += select(vec3<f32>(0.0), reflection, has(ENV) && has(REFLECTION_TEXTURE));

    var out: FragmentOutput;
#ifdef BLEND_MULTIPLY
    // dif.spec.decal.mult (dif-spec-mult §6.2): the lit colour multiplies
    // the target and alpha does not fade it; Bevy's multiply blend fades by
    // 1 − α, so α is 1. The target already holds the output scale, so the
    // multiplier is the game's lit colour unscaled.
    out.color = vec4<f32>(lit, 1.0);
#else
    out.color = vec4<f32>(to_output(light, lit), output_alpha);
#endif
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
