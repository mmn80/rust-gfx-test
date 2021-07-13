use crate::{
    assets::mesh::PER_MATERIAL_DESCRIPTOR_SET_LAYOUT_INDEX,
    features::mesh::MeshUntexturedRenderFeatureFlag,
    phases::{DepthPrepassRenderPhase, OpaqueRenderPhase, WireframeRenderPhase},
};
use rafx::{
    api::RafxIndexType,
    assets::MaterialInstanceAsset,
    framework::{BufferResource, DescriptorSetArc, MaterialPassResource, ResourceArc},
    rafx_visibility::VisibleBounds,
    render_feature_extract_job_predule::{
        RenderPhase, RenderPhaseIndex, RenderView, RwLock, RwLockReadGuard, RwLockWriteGuard,
    },
};
use std::sync::Arc;

use super::MeshRenderObject;

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

impl DynMeshPart {
    pub fn new(
        material_instance: MaterialInstanceAsset,
        vertex_buffer_offset_in_bytes: u32,
        vertex_buffer_size_in_bytes: u32,
        index_buffer_offset_in_bytes: u32,
        index_buffer_size_in_bytes: u32,
        index_type: RafxIndexType,
    ) -> Self {
        let textured_pass_index = material_instance
            .material
            .find_pass_by_name("mesh textured")
            .expect("could not find `mesh textured` pass in mesh part material");

        let textured_z_pass_index = material_instance
            .material
            .find_pass_by_name("mesh textured z")
            .expect("could not find `mesh textured z` pass in mesh part material");

        assert_eq!(
            textured_z_pass_index,
            textured_pass_index + 1,
            "expected `mesh textured z` to occur after `mesh textured`"
        );

        let untextured_pass_index = material_instance
            .material
            .find_pass_by_name("mesh untextured")
            .expect("could not find `mesh untextured` pass in mesh part material");

        let untextured_z_pass_index = material_instance
            .material
            .find_pass_by_name("mesh untextured z")
            .expect("could not find `mesh untextured z` pass in mesh part material");

        assert_eq!(
            untextured_z_pass_index,
            untextured_pass_index + 1,
            "expected `mesh untextured z` to occur after `mesh untextured`"
        );

        let wireframe_pass_index = material_instance
            .material
            .find_pass_by_name("mesh wireframe")
            .expect("could not find `mesh wireframe` pass in mesh part material");

        DynMeshPart {
            material_instance: material_instance.clone(),
            textured_pass_index,
            untextured_pass_index,
            wireframe_pass_index,
            vertex_buffer_offset_in_bytes,
            vertex_buffer_size_in_bytes,
            index_buffer_offset_in_bytes,
            index_buffer_size_in_bytes,
            index_type,
        }
    }

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
    pub mesh_parts: Vec<DynMeshPart>,
    pub vertex_buffer: ResourceArc<BufferResource>,
    pub index_buffer: ResourceArc<BufferResource>,
    pub visible_bounds: VisibleBounds,
}

#[derive(Clone)]
pub struct DynMesh {
    pub inner: Arc<DynMeshInner>,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub struct DynMeshHandle(u32);

pub struct DynMeshCacheInner {
    meshes: Vec<DynMesh>,
    render_objects: Vec<MeshRenderObject>,
}

#[derive(Clone)]
pub struct DynMeshCache {
    inner: Arc<RwLock<DynMeshCacheInner>>,
}

impl DynMeshCache {
    pub fn new() -> Self {
        DynMeshCache {
            inner: Arc::new(RwLock::new(DynMeshCacheInner {
                meshes: Vec::new(),
                render_objects: Vec::new(),
            })),
        }
    }

    pub fn register_dyn_mesh(&mut self, mesh: DynMesh) -> DynMeshHandle {
        let mut inner = self.write();
        let dyn_mesh_id = DynMeshHandle {
            0: inner.meshes.len() as u32,
        };
        inner.meshes.push(mesh);
        inner
            .render_objects
            .push(MeshRenderObject::DynMeshHandle(dyn_mesh_id));
        dyn_mesh_id
    }

    pub fn update_dyn_mesh(&mut self, handle: DynMeshHandle, mesh: DynMesh) {
        let mut inner = self.write();
        assert!(handle.0 < inner.meshes.len() as u32);
        let _ = std::mem::replace(&mut inner.meshes[handle.0 as usize], mesh);
    }

    pub fn get_dyn_mesh(&self, handle: DynMeshHandle) -> DynMesh {
        let inner = self.read();
        assert!(handle.0 < inner.meshes.len() as u32);
        inner.meshes[handle.0 as usize].clone()
    }

    pub fn free_dyn_mesh(&mut self, handle: DynMeshHandle) {
        let mut inner = self.write();
        assert!(handle.0 < inner.meshes.len() as u32);
        inner.meshes.remove(handle.0 as usize);
        inner.render_objects.remove(handle.0 as usize);
    }

    fn write(&mut self) -> RwLockWriteGuard<DynMeshCacheInner> {
        let inner = &self.inner;
        inner.try_write().unwrap_or_else(move || {
            log::warn!("DynMeshCacheInner is being read or written by another thread.");
            inner.write()
        })
    }

    fn read(&self) -> RwLockReadGuard<DynMeshCacheInner> {
        let inner = &self.inner;
        inner.try_read().unwrap_or_else(move || {
            log::warn!("DynMeshCacheInner is being written by another thread.");
            inner.read()
        })
    }
}
