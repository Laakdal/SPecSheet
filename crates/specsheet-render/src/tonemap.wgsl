// The game's tonemap pass, written from docs/app/shader-specs/tonemap.md §4
// (D13): low-light tint, exposure, colour correction (`col`), and the tone
// curve with its hue handling. Linear in, linear display colour out; the
// target's sRGB encoding does the rest (§4.6). No bloom yet.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

// Mirrors `GameTonemap` in tonemap.rs; the symbols are the spec's.
struct GameTonemap {
    t0: vec4<f32>,              // k multiplier, unused, W, per-channel blend
    curve: array<vec4<f32>, 5>, // T1…T5
    low_intensity: vec4<f32>,   // a, b
    low_intensity_color: vec4<f32>,
    color_correction: vec4<f32>, // balance B, saturation S
    exposure: f32,               // E
    color_correct: u32,
}

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> settings: GameTonemap;

// ε = 2^−24, the spec's small-value floor.
const EPSILON: f32 = 5.9604645e-8;

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126729, 0.7151522, 0.0721750));
}

// F(x), §4.4: three power segments in log2 space.
fn curve(x: f32) -> f32 {
    let c = settings.curve;
    let t = log2(max(x, EPSILON));
    var ab = c[3].xy;
    var gd = c[3].zw;
    var mn = c[4].xy;
    if t < c[4].z {
        ab = c[0].xy;
        gd = c[0].zw;
        mn = c[1].xy;
    } else if t <= c[4].w {
        ab = c[1].zw;
        gd = c[2].xy;
        mn = c[2].zw;
    }
    let u = max(ab.x * t + ab.y, EPSILON);
    let v = exp2(gd.x + gd.y * log2(u));
    return exp2(mn.x * v + mn.y);
}

// §4.3: a saturation lerp, then a mid-tone balance in square-root space
// that keeps the luminance.
fn color_correct(c2: vec3<f32>) -> vec3<f32> {
    let balance = settings.color_correction.xyz;
    let y2 = luminance(c2);
    // u ≥ 0 guards S > 1, where the square root would be NaN.
    let u = max(mix(vec3<f32>(y2), c2, settings.color_correction.w), vec3<f32>(0.0));
    let v = sqrt(u);
    let d = 2.0 * v - 1.0;
    let corrected = max(v + saturate(1.0 - d * d) * balance, vec3<f32>(0.0));
    let squared = corrected * corrected;
    return squared * y2 / max(luminance(squared), 1e-5);
}

// §4.5: the curve on luminance; colours brighter than their hue allows
// desaturate towards a hue-dependent white.
fn tone_map(c3: vec3<f32>) -> vec3<f32> {
    let w = settings.t0.z;
    let c = max(c3, vec3<f32>(EPSILON));
    let y = luminance(c);
    let top = max(c.r, max(c.g, c.b));
    let bottom = min(c.r, min(c.g, c.b));
    let target_luminance = curve(y);
    let n = c * w / top;
    let yn = y * w / top;
    if yn >= target_luminance {
        return n * target_luminance / yn;
    }
    let x = saturate((target_luminance - yn) / (w - yn));
    let sigma = saturate((top - bottom) / top);
    let g = mix(x, pow(x, 16.0), saturate(yn / w)) * saturate(1.0 - 0.65 * sigma * sigma);
    let rho = (c - bottom) / top;
    let red = saturate(rho.r - rho.g - rho.b);
    let green = saturate(rho.g - rho.r - rho.b);
    var tint = vec3<f32>(0.6, 1.0, 0.4);
    var weight = green;
    if red > 0.0 {
        tint = vec3<f32>(1.0, 0.4, 0.35);
        weight = red;
    }
    let white = mix(vec3<f32>(w), w * tint, weight);
    let desaturated = mix(n, white, g);
    let per_channel = vec3<f32>(curve(c.r), curve(c.g), curve(c.b));
    return mix(desaturated, per_channel, settings.t0.w * x);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let scene = textureSample(screen_texture, screen_sampler, in.uv).rgb;
    // §4.2 in order.
    let c0 = max(scene, vec3<f32>(0.0));
    let y0 = luminance(c0);
    let f = saturate((y0 - settings.low_intensity.x) * settings.low_intensity.y);
    let c1 = mix(settings.low_intensity_color.rgb * y0, c0, f);
    let c2 = settings.t0.x * settings.exposure * c1;
    var c3 = c2;
    if settings.color_correct != 0u {
        c3 = color_correct(c2);
    }
    return vec4<f32>(tone_map(c3), 1.0);
}
