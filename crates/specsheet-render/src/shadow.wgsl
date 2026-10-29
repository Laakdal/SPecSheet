// `shadowonly` geometry has no colour pass (shadows-and-fake-lights.md §1):
// every fragment is discarded; Bevy's shadow pass draws its depth.

#import bevy_pbr::forward_io::VertexOutput

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    discard;
    return vec4<f32>(0.0);
}
