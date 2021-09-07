use std::sync::Arc;

use rafx::{
    api::RafxIndexType,
    assets::MaterialInstanceAsset,
    framework::{BufferResource, DescriptorSetArc, MaterialPassResource, ResourceArc},
    rafx_visibility::VisibleBounds,
    render_features::{RenderPhase, RenderPhaseIndex, RenderView},
};
use rafx_plugins::{
    features::mesh::MeshUntexturedRenderFeatureFlag,
    phases::{DepthPrepassRenderPhase, OpaqueRenderPhase, WireframeRenderPhase},
};

pub use super::buffer_upload::BufferUploaderConfig;

#[derive(Clone)]
pub struct DynMeshDataPart {
    pub material_instance: MaterialInstanceAsset,
    pub vertex_buffer_offset_in_bytes: u32,
    pub vertex_buffer_size_in_bytes: u32,
    pub index_buffer_offset_in_bytes: u32,
    pub index_buffer_size_in_bytes: u32,
    pub index_type: RafxIndexType,
}

#[derive(Clone)]
pub struct DynMeshData {
    pub mesh_parts: Vec<DynMeshDataPart>,
    pub vertex_buffer: Option<Vec<u8>>,
    pub index_buffer: Option<Vec<u8>>,
    pub visible_bounds: VisibleBounds,
}

impl std::fmt::Display for DynMeshData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let vtx_sz = self.vertex_buffer.as_ref().unwrap().len();
        let idx_sz = self.index_buffer.as_ref().unwrap().len();
        let vtx_q = 4 * std::mem::size_of::<rafx_plugins::features::mesh::MeshVertex>() as u32;
        let idx_q = 6 * std::mem::size_of::<u16>() as u32;
        write!(
            f,
            "vx_all: {} ({}q); ix_all: {} (~{}q); parts: {}",
            vtx_sz,
            vtx_sz / (vtx_q as usize),
            idx_sz,
            idx_sz / (idx_q as usize),
            itertools::Itertools::join(
                &mut self.mesh_parts.iter().map(|p| {
                    let idx_q = 6
                        * (match p.index_type {
                            rafx::api::RafxIndexType::Uint32 => std::mem::size_of::<u32>(),
                            rafx::api::RafxIndexType::Uint16 => std::mem::size_of::<u16>(),
                        }) as u32;
                    format!(
                        "vx: [{} ({}q) += {} ({}q)], ix: [{} ({}q) += {} ({}q)]",
                        p.vertex_buffer_offset_in_bytes,
                        p.vertex_buffer_offset_in_bytes / vtx_q,
                        p.vertex_buffer_size_in_bytes,
                        p.vertex_buffer_size_in_bytes / vtx_q,
                        p.index_buffer_offset_in_bytes,
                        p.index_buffer_offset_in_bytes / idx_q,
                        p.index_buffer_size_in_bytes,
                        p.index_buffer_size_in_bytes / idx_q
                    )
                }),
                ", "
            )
        )
    }
}

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
    pub visible_bounds: VisibleBounds,
}

#[derive(Clone)]
pub struct DynMesh {
    pub inner: Arc<DynMeshInner>,
}
