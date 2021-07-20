use crate::{
    features::mesh::MeshUntexturedRenderFeatureFlag,
    phases::{DepthPrepassRenderPhase, OpaqueRenderPhase, WireframeRenderPhase},
};
use rafx::{
    api::RafxIndexType,
    assets::MaterialInstanceAsset,
    framework::{BufferResource, DescriptorSetArc, MaterialPassResource, ResourceArc},
    render_feature_extract_job_predule::*,
};
use std::sync::Arc;

pub struct DynMeshPart {
    pub material_instance: MaterialInstanceAsset,
    pub textured_pass_index: usize,
    pub untextured_pass_index: usize,
    pub wireframe_pass_index: usize,
    pub vertex_buffer_offset_in_bytes: u32,
    pub vertex_buffer_size_in_bytes: u32,
    pub index_buffer_offset_in_bytes: u32,
    pub index_buffer_size_in_bytes: u32,
    pub index_type: RafxIndexType,
}

pub const PER_MATERIAL_DESCRIPTOR_SET_LAYOUT_INDEX: usize = 1;

impl DynMeshPart {
    pub fn get_material_pass_index(
        &self,
        view: &RenderView,
        render_phase_index: RenderPhaseIndex,
    ) -> usize {
        if render_phase_index == OpaqueRenderPhase::render_phase_index() {
            let offset = !view.phase_is_relevant::<DepthPrepassRenderPhase>() as usize;
            return if view.feature_flag_is_relevant::<MeshUntexturedRenderFeatureFlag>() {
                self.untextured_pass_index + offset
            } else {
                self.textured_pass_index + offset
            };
        } else if render_phase_index == WireframeRenderPhase::render_phase_index() {
            self.wireframe_pass_index
        } else {
            panic!(
                "mesh does not support render phase index {}",
                render_phase_index
            )
        }
    }

    pub fn get_material_pass_resource(
        &self,
        view: &RenderView,
        render_phase_index: RenderPhaseIndex,
    ) -> &ResourceArc<MaterialPassResource> {
        &self.material_instance.material.passes
            [self.get_material_pass_index(view, render_phase_index)]
        .material_pass_resource
    }

    pub fn get_material_descriptor_set(
        &self,
        view: &RenderView,
        render_phase_index: RenderPhaseIndex,
    ) -> &DescriptorSetArc {
        return &self.material_instance.material_descriptor_sets
            [self.get_material_pass_index(view, render_phase_index)]
            [PER_MATERIAL_DESCRIPTOR_SET_LAYOUT_INDEX]
            .as_ref()
            .unwrap();
    }
}

pub struct DynMeshInner {
    pub mesh_parts: Vec<Option<DynMeshPart>>,
    pub vertex_buffer: ResourceArc<BufferResource>,
    pub index_buffer: ResourceArc<BufferResource>,
}

#[derive(Clone)]
pub struct DynMesh {
    pub inner: Arc<DynMeshInner>,
}

#[derive(Clone)]
pub struct DynMeshHandle(u32);

impl DynMeshHandle {
    pub fn get_dyn_mesh(&self) -> Option<DynMesh> {
        todo!();
    }
}
