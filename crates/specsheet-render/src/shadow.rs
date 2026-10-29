//! `shadowonly` geometry (shadows-and-fake-lights.md §1): never drawn in
//! colour, only into the sun's shadow map, with front faces culled. The
//! colour pass discards every fragment; Bevy's shadow pass draws the depth.

use bevy::asset::embedded_asset;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, Face, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;

const SHADER: &str = "embedded://specsheet_render/shadow.wgsl";

pub struct ShadowOnlyPlugin;

impl Plugin for ShadowOnlyPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shadow.wgsl");
        app.add_plugins(MaterialPlugin::<ShadowOnlyMaterial>::default());
    }

    fn finish(&self, app: &mut App) {
        app.init_resource::<ShadowOnly>();
    }
}

/// The material of every `shadowonly` piece.
#[derive(Asset, TypePath, AsBindGroup, Clone, Default)]
pub struct ShadowOnlyMaterial {}

impl Material for ShadowOnlyMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // The game culls front faces; its `nocull` flavour culls none (ours:
        // both cull front faces).
        descriptor.primitive.cull_mode = Some(Face::Front);
        Ok(())
    }
}

/// The one shared [`ShadowOnlyMaterial`].
#[derive(Resource)]
pub struct ShadowOnly(pub Handle<ShadowOnlyMaterial>);

impl FromWorld for ShadowOnly {
    fn from_world(world: &mut World) -> Self {
        Self(
            world
                .resource_mut::<Assets<ShadowOnlyMaterial>>()
                .add(ShadowOnlyMaterial::default()),
        )
    }
}
