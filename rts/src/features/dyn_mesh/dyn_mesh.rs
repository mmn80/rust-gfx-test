use super::buffer_upload::BufferUploader;
pub use super::buffer_upload::BufferUploaderConfig;
use distill::loader::handle::Handle;
use rafx::{
    api::{RafxDeviceContext, RafxIndexType, RafxQueue},
    assets::MaterialInstanceAsset,
    base::slab::{DropSlab, GenericDropSlabKey},
    framework::{BufferResource, DescriptorSetArc, MaterialPassResource, ResourceArc},
    rafx_visibility::VisibleBounds,
    render_feature_extract_job_predule::*,
};
use rafx_plugins::{
    features::mesh::{MeshUntexturedRenderFeatureFlag, MeshVertex},
    phases::{DepthPrepassRenderPhase, OpaqueRenderPhase, WireframeRenderPhase},
};
use std::sync::Arc;

pub struct DynMeshDataPart {
    pub material_instance: Handle<MaterialInstanceAsset>,
    pub vertex_buffer_offset_in_bytes: u32,
    pub vertex_buffer_size_in_bytes: u32,
    pub index_buffer_offset_in_bytes: u32,
    pub index_buffer_size_in_bytes: u32,
    pub index_type: RafxIndexType,
}

pub struct DynMeshDataInner {
    pub mesh_parts: Vec<Option<DynMeshDataPart>>,
    pub vertex_buffer: Vec<MeshVertex>,
    pub index_buffer: Vec<u16>,
    pub visible_bounds: VisibleBounds,
}

#[derive(Clone)]
pub struct DynMeshData {
    pub inner: Arc<DynMeshDataInner>,
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

#[derive(Clone)]
pub enum DynMeshState {
    Registered(DynMeshData),
    Uploading(DynMeshData),
    Uploaded(DynMesh),
}

#[derive(Clone, Debug)]
pub struct DynMeshHandle {
    handle: GenericDropSlabKey,
}

pub struct DynMeshStorage {
    storage: DropSlab<DynMeshState>,
    upload: Option<BufferUploader>,
}

impl DynMeshStorage {
    pub fn new() -> Self {
        Self {
            storage: Default::default(),
            upload: None,
        }
    }

    pub fn init_buffer_uploader(
        &mut self,
        device_context: &RafxDeviceContext,
        upload_queue_config: BufferUploaderConfig,
        graphics_queue: RafxQueue,
        transfer_queue: RafxQueue,
    ) {
        if self.upload.is_none() {
            self.upload = Some(BufferUploader::new(
                device_context,
                upload_queue_config,
                graphics_queue,
                transfer_queue,
            ))
        } else {
            log::error!("BufferUploadManager already initialized");
        }
    }

    pub fn register_dyn_mesh(&mut self, dyn_mesh_data: DynMeshData) -> DynMeshHandle {
        self.storage.process_drops();

        let dyn_mesh = DynMeshState::Registered(dyn_mesh_data);

        let drop_slab_key = self.storage.allocate(dyn_mesh);
        DynMeshHandle {
            handle: drop_slab_key.generic_drop_slab_key(),
        }
    }

    pub fn get(&self, dyn_mesh_handle: &DynMeshHandle) -> &DynMeshState {
        self.storage
            .get(&dyn_mesh_handle.handle.drop_slab_key())
            .unwrap_or_else(|| {
                panic!(
                    "DynMeshStorage did not contain handle {:?}.",
                    dyn_mesh_handle
                )
            })
    }

    pub fn get_mut(&mut self, dyn_mesh_handle: &DynMeshHandle) -> &mut DynMeshState {
        self.storage
            .get_mut(&dyn_mesh_handle.handle.drop_slab_key())
            .unwrap_or_else(|| {
                panic!(
                    "DynMeshStorage did not contain handle {:?}.",
                    dyn_mesh_handle
                )
            })
    }
}

#[derive(Clone)]
pub struct DynMeshResource {
    storage: Arc<RwLock<DynMeshStorage>>,
}

impl DynMeshResource {
    pub fn new() -> Self {
        Self {
            storage: Arc::new(RwLock::new(DynMeshStorage::new())),
        }
    }

    pub fn read(&self) -> RwLockReadGuard<DynMeshStorage> {
        let registry = &self.storage;
        registry.try_read().unwrap_or_else(move || {
            log::warn!("DynMeshStorage is being written by another thread.");

            registry.read()
        })
    }

    fn write(&mut self) -> RwLockWriteGuard<DynMeshStorage> {
        let registry = &self.storage;
        registry.try_write().unwrap_or_else(move || {
            log::warn!("DynMeshStorage is being read or written by another thread.");

            registry.write()
        })
    }

    pub fn init_buffer_uploader(
        &mut self,
        device_context: &RafxDeviceContext,
        upload_queue_config: BufferUploaderConfig,
        graphics_queue: RafxQueue,
        transfer_queue: RafxQueue,
    ) {
        let mut storage = self.write();
        storage.init_buffer_uploader(
            device_context,
            upload_queue_config,
            graphics_queue,
            transfer_queue,
        );
    }

    pub fn update_buffer_uploads(&mut self) {
        let mut storage = self.write();
        if let Some(ref mut upload) = storage.upload {
            let _res = upload.update();
        }
    }

    pub fn register_dyn_mesh(&mut self, dyn_mesh_data: DynMeshData) -> DynMeshHandle {
        let dyn_mesh_handle = {
            let mut storage = self.write();
            storage.register_dyn_mesh(dyn_mesh_data)
        };

        dyn_mesh_handle
    }

    pub fn update_dyn_mesh(&mut self, dyn_mesh_handle: &DynMeshHandle, dyn_mesh_data: DynMeshData) {
        let dyn_mesh = DynMeshState::Registered(dyn_mesh_data);
        let mut storage = self.write();
        let old_dyn_mesh = storage.get_mut(dyn_mesh_handle);
        let _old = std::mem::replace(old_dyn_mesh, dyn_mesh);
    }
}
