// The game's forward lighting, shared by the clean-room materials, written
// from docs/app/shader-specs/dif-spec.md §5 and truckpaint-lighting.md §5
// (D13). Vectors are in world space; the spec's eye-space maths is the same
// except for the reflection lookup, which then needs no rotation.

#define_import_path specsheet::game_lighting

// Mirrors `GameLight` in lighting.rs.
struct GameLight {
    to_sun: vec4<f32>,             // L, unit, towards the sun
    sun_diffuse: vec4<f32>,        // Csun_d
    sun_specular: vec4<f32>,       // Csun_s
    ambient: vec4<f32>,            // Camb; w: Aenv, the reflection visibility
    additional_ambient: vec4<f32>, // Cadd; w: our scale into the tonemap pass
}

// The sun and ambient terms of dif-spec §5.4: Ld and Ls.
struct SunLight {
    diffuse: vec3<f32>,
    specular: vec3<f32>,
}

// E: unit vector from the surface towards the eye, for P = surface − eye.
fn eye_vector(p: vec3<f32>) -> vec3<f32> {
    return -p / sqrt(max(dot(p, p), 1e-6));
}

// Blinn-Phong without normalisation. The gate is sat(10·N·L), or N·L for
// the `weight` flavour.
fn sun_light(light: GameLight, n: vec3<f32>, e: vec3<f32>, power: f32, weight_gate: bool) -> SunLight {
    let l = light.to_sun.xyz;
    let nl = saturate(dot(n, l));
    let nh = saturate(dot(n, normalize(e + l)));
    let lobe = select(0.0, pow(nh, power), nh > 0.0);
    let gate = select(saturate(10.0 * nl), nl, weight_gate);
    var out: SunLight;
    out.diffuse = light.ambient.rgb + light.sun_diffuse.rgb * nl;
    out.specular = light.sun_specular.rgb * lobe * gate;
    return out;
}

// sat(Aenv), the forward pass's reflection visibility.
fn reflection_visibility(light: GameLight) -> f32 {
    return saturate(light.ambient.w);
}

// The reflection of P about N, with the far stretch of the vertical
// coordinate (factor 1 at 1 m, 21 at 300 m), turned into the direction Bevy
// samples its cube images with (z flipped, as its environment maps do).
fn reflection_lookup(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    var d = p - 2.0 * dot(n, p) * n;
    d.y = d.y * (1.0 + 20.0 * saturate((dot(p, p) - 1.0) / 90000.0));
    return vec3<f32>(d.x, d.y, -d.z);
}

// Truck paint's Schlick fresnel from one value.
fn schlick_fresnel(f0: f32, n_dot_e: f32) -> f32 {
    return saturate(f0 + (1.0 - f0) * pow(1.0 - saturate(n_dot_e), 5.0));
}

// dif.spec's linear fresnel from (bias, scale); N·E is not clamped first.
fn linear_fresnel(bias: f32, scale: f32, n_dot_e: f32) -> f32 {
    return saturate(bias + scale * (1.0 - n_dot_e));
}

// The game writes linear HDR; this is the one scale into the tonemap pass.
fn to_output(light: GameLight, colour: vec3<f32>) -> vec3<f32> {
    return colour * light.additional_ambient.w;
}
