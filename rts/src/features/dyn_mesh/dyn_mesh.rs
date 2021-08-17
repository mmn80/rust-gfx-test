pub use super::buffer_upload::BufferUploaderConfig;
use super::buffer_upload::{BufferUploadId, BufferUploadResult, BufferUploader};
use crossbeam_channel::{Receiver, Sender};
use fnv::FnvHashMap;
use rafx::{
    api::{RafxBuffer, RafxDeviceContext, RafxError, RafxIndexType, RafxQueue, RafxResourceType},
    assets::MaterialInstanceAsset,
    base::slab::{DropSlab, GenericDropSlabKey},
    framework::{BufferResource, DescriptorSetArc, MaterialPassResource, ResourceArc},
    rafx_visibility::VisibleBounds,
    render_feature_extract_job_predule::*,
    render_feature_renderer_prelude::AssetManager,
    render_feature_write_job_prelude::RafxResult,
};
use rafx_plugins::{
    features::mesh::MeshUntexturedRenderFeatureFlag,
    phases::{DepthPrepassRenderPhase, OpaqueRenderPhase, WireframeRenderPhase},
};
use std::sync::Arc;

pub struct DynMeshDataPart {
    pub material_instance: MaterialInstanceAsset,
    pub vertex_buffer_offset_in_bytes: u32,
    pub vertex_buffer_size_in_bytes: u32,
    pub index_buffer_offset_in_bytes: u32,
    pub index_buffer_size_in_bytes: u32,
    pub index_type: RafxIndexType,
}

pub struct DynMeshData {
    pub mesh_parts: Vec<DynMeshDataPart>,
    pub vertex_buffer: Option<Vec<u8>>,
    pub index_buffer: Option<Vec<u8>>,
    pub visible_bounds: VisibleBounds,
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

struct DynMeshUpload {
    pub mesh_data: DynMeshData,
    pub vertex_upload_id: BufferUploadId,
    pub vertex_rx: Receiver<BufferUploadResult>,
    pub vertex_buffer: Option<RafxBuffer>,
    pub index_upload_id: BufferUploadId,
    pub index_rx: Receiver<BufferUploadResult>,
    pub index_buffer: Option<RafxBuffer>,
}

enum DynMeshState {
    Uploading(DynMeshUpload, Option<DynMesh>),
    Completed(DynMesh),
    UploadError,
}

#[derive(Clone, Debug)]
pub struct DynMeshHandle {
    key: GenericDropSlabKey,
}

struct DynMeshStorage {
    storage: DropSlab<DynMeshState>,
    uploader: Option<BufferUploader>,
    vertex_uploads: FnvHashMap<BufferUploadId, DynMeshHandle>,
    vertex_tx: Sender<BufferUploadResult>,
    vertex_rx: Receiver<BufferUploadResult>,
    index_uploads: FnvHashMap<BufferUploadId, DynMeshHandle>,
    index_tx: Sender<BufferUploadResult>,
    index_rx: Receiver<BufferUploadResult>,
}

impl DynMeshStorage {
    pub fn new() -> Self {
        let (vertex_tx, vertex_rx) = crossbeam_channel::unbounded();
        let (index_tx, index_rx) = crossbeam_channel::unbounded();
        Self {
            storage: Default::default(),
            uploader: None,
            vertex_uploads: Default::default(),
            vertex_tx,
            vertex_rx,
            index_uploads: Default::default(),
            index_tx,
            index_rx,
        }
    }

    pub fn init_buffer_uploader(
        &mut self,
        device_context: &RafxDeviceContext,
        upload_queue_config: BufferUploaderConfig,
        graphics_queue: RafxQueue,
        transfer_queue: RafxQueue,
    ) {
        if self.uploader.is_none() {
            self.uploader = Some(BufferUploader::new(
                device_context,
                upload_queue_config,
                graphics_queue,
                transfer_queue,
            ))
        } else {
            log::error!("BufferUploadManager already initialized");
        }
    }

    pub fn start_upload(
        &mut self,
        mut mesh_data: DynMeshData,
        handle: Option<&DynMeshHandle>,
    ) -> RafxResult<DynMeshState> {
        if mesh_data.vertex_buffer.is_none() || mesh_data.index_buffer.is_none() {
            return Err(RafxError::StringError(
                "Dyn mesh data does not contain data".to_string(),
            ));
        }
        let uploader = self.uploader.as_ref().unwrap();
        let vertex_data = std::mem::take(&mut mesh_data.vertex_buffer).unwrap();
        let vertex_upload_id = uploader.upload_buffer(
            RafxResourceType::VERTEX_BUFFER,
            vertex_data,
            self.vertex_tx.clone(),
        )?;
        let index_data = std::mem::take(&mut mesh_data.index_buffer).unwrap();
        let index_upload_id = uploader.upload_buffer(
            RafxResourceType::INDEX_BUFFER,
            index_data,
            self.index_tx.clone(),
        )?;

        let old_dyn_mash = handle.and_then(|handle| {
            if let DynMeshState::Completed(dyn_mesh) = self.get(handle) {
                Some(dyn_mesh.clone())
            } else {
                None
            }
        });

        Ok(DynMeshState::Uploading(
            DynMeshUpload {
                mesh_data,
                vertex_upload_id,
                vertex_rx: self.vertex_rx.clone(),
                vertex_buffer: None,
                index_upload_id,
                index_rx: self.index_rx.clone(),
                index_buffer: None,
            },
            old_dyn_mash,
        ))
    }

    pub fn process_upload_results(&mut self, asset_manager: &mut AssetManager) {
        for upload_result in self.vertex_rx.try_iter().collect::<Vec<_>>() {
            let (upload_id, buffer) = match upload_result {
                BufferUploadResult::UploadError(upload_id) => (upload_id, None),
                BufferUploadResult::UploadDrop(upload_id) => (upload_id, None),
                BufferUploadResult::UploadComplete(upload_id, buffer) => (upload_id, Some(buffer)),
            };
            let handle = self.vertex_uploads.get(&upload_id).unwrap().clone();
            if let (Some(buffer), DynMeshState::Uploading(ref mut upload, _)) =
                (buffer, self.get_mut(&handle))
            {
                upload.vertex_buffer = Some(buffer);
            } else {
                log::error!(
                    "Vertex buffer upload error (upload id: {:?}) for dyn mesh: {:?}",
                    upload_id,
                    handle
                );
                let _old = std::mem::replace(self.get_mut(&handle), DynMeshState::UploadError);
            }
            self.vertex_uploads.remove(&upload_id);
            self.check_finished_upload(&handle, asset_manager);
        }
        for upload_result in self.index_rx.try_iter().collect::<Vec<_>>() {
            let (upload_id, buffer) = match upload_result {
                BufferUploadResult::UploadError(upload_id) => (upload_id, None),
                BufferUploadResult::UploadDrop(upload_id) => (upload_id, None),
                BufferUploadResult::UploadComplete(upload_id, buffer) => (upload_id, Some(buffer)),
            };
            let handle = self.vertex_uploads.get(&upload_id).unwrap().clone();
            if let (Some(buffer), DynMeshState::Uploading(ref mut upload, _)) =
                (buffer, self.get_mut(&handle))
            {
                upload.index_buffer = Some(buffer);
            } else {
                log::error!(
                    "Index buffer upload error (upload id: {:?}) for dyn mesh: {:?}",
                    upload_id,
                    handle
                );
                let _old = std::mem::replace(self.get_mut(&handle), DynMeshState::UploadError);
            }
            self.index_uploads.remove(&upload_id);
            self.check_finished_upload(&handle, asset_manager);
        }
    }

    fn check_finished_upload(&mut self, handle: &DynMeshHandle, asset_manager: &mut AssetManager) {
        let mesh_state = self.get_mut(handle);
        if let DynMeshState::Uploading(upload, _) = mesh_state {
            if let (Some(vertex_buffer), Some(index_buffer)) =
                (upload.vertex_buffer.take(), upload.index_buffer.take())
            {
                let visible_bounds = upload.mesh_data.visible_bounds;
                let vertex_buffer = asset_manager.resources().insert_buffer(vertex_buffer);
                let index_buffer = asset_manager.resources().insert_buffer(index_buffer);
                let mesh_parts: Vec<_> = upload
                    .mesh_data
                    .mesh_parts
                    .iter()
                    .map(|mesh_part| {
                        let material_instance = mesh_part.material_instance.clone();

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
                            .expect(
                                "could not find `mesh untextured z` pass in mesh part material",
                            );

                        assert_eq!(
                            untextured_z_pass_index,
                            untextured_pass_index + 1,
                            "expected `mesh untextured z` to occur after `mesh untextured`"
                        );

                        let wireframe_pass_index = material_instance
                            .material
                            .find_pass_by_name("mesh wireframe")
                            .expect("could not find `mesh wireframe` pass in mesh part material");

                        Some(DynMeshPart {
                            material_instance,
                            textured_pass_index,
                            untextured_pass_index,
                            wireframe_pass_index,
                            vertex_buffer_offset_in_bytes: mesh_part.vertex_buffer_offset_in_bytes,
                            vertex_buffer_size_in_bytes: mesh_part.vertex_buffer_size_in_bytes,
                            index_buffer_offset_in_bytes: mesh_part.index_buffer_offset_in_bytes,
                            index_buffer_size_in_bytes: mesh_part.index_buffer_size_in_bytes,
                            index_type: mesh_part.index_type,
                        })
                    })
                    .collect();

                let inner = DynMeshInner {
                    vertex_buffer,
                    index_buffer,
                    mesh_parts,
                    visible_bounds,
                };
                let dyn_mesh = DynMesh {
                    inner: Arc::new(inner),
                };

                let _old = std::mem::replace(mesh_state, DynMeshState::Completed(dyn_mesh));
            }
        }
    }

    pub fn add_dyn_mesh(&mut self, mesh_data: DynMeshData) -> RafxResult<DynMeshHandle> {
        let mesh_state = self.start_upload(mesh_data, None)?;

        self.storage.process_drops();
        let drop_slab_key = self.storage.allocate(mesh_state);
        let handle = DynMeshHandle {
            key: drop_slab_key.generic_drop_slab_key(),
        };

        let mesh_state = self.storage.get(&drop_slab_key).unwrap();
        if let DynMeshState::Uploading(upload, _) = mesh_state {
            self.vertex_uploads
                .insert(upload.vertex_upload_id.clone(), handle.clone());
            self.index_uploads
                .insert(upload.index_upload_id.clone(), handle.clone());
        } else {
            unreachable!();
        }

        Ok(handle)
    }

    pub fn get(&self, handle: &DynMeshHandle) -> &DynMeshState {
        self.storage
            .get(&handle.key.drop_slab_key())
            .unwrap_or_else(|| panic!("DynMeshStorage did not contain handle {:?}.", handle))
    }

    pub fn get_mut(&mut self, handle: &DynMeshHandle) -> &mut DynMeshState {
        self.storage
            .get_mut(&handle.key.drop_slab_key())
            .unwrap_or_else(|| panic!("DynMeshStorage did not contain handle {:?}.", handle))
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

    fn read(&self) -> RwLockReadGuard<DynMeshStorage> {
        let storage = &self.storage;
        storage.try_read().unwrap_or_else(move || {
            log::warn!("DynMeshStorage is being written by another thread.");
            storage.read()
        })
    }

    fn write(&mut self) -> RwLockWriteGuard<DynMeshStorage> {
        let storage = &self.storage;
        storage.try_write().unwrap_or_else(move || {
            log::warn!("DynMeshStorage is being read or written by another thread.");
            storage.write()
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

    pub fn update(&mut self, asset_manager: &mut AssetManager) {
        let mut storage = self.write();
        if let Some(ref mut upload) = storage.uploader {
            let _res = upload.update();
        }
        storage.process_upload_results(asset_manager);
    }

    pub fn add_dyn_mesh(&mut self, mesh_data: DynMeshData) -> RafxResult<DynMeshHandle> {
        let handle = {
            let mut storage = self.write();
            storage.add_dyn_mesh(mesh_data)
        };

        handle
    }

    pub fn update_dyn_mesh(
        &mut self,
        handle: &DynMeshHandle,
        mesh_data: DynMeshData,
    ) -> RafxResult<()> {
        let mut storage = self.write();
        let mesh_state = storage.start_upload(mesh_data, Some(handle))?;

        if let DynMeshState::Uploading(ref upload, _) = mesh_state {
            storage
                .vertex_uploads
                .insert(upload.vertex_upload_id.clone(), handle.clone());
            storage
                .index_uploads
                .insert(upload.index_upload_id.clone(), handle.clone());
        } else {
            unreachable!();
        }

        let old_mesh_state = storage.get_mut(handle);
        let _old = std::mem::replace(old_mesh_state, mesh_state);

        Ok(())
    }

    pub fn get(&self, handle: &DynMeshHandle) -> Option<DynMesh> {
        let storage = self.read();
        match storage.get(handle) {
            DynMeshState::Uploading(_, old_dyn_mesh) => old_dyn_mesh.clone(),
            DynMeshState::Completed(mesh) => Some(mesh.clone()),
            DynMeshState::UploadError => None,
        }
    }
}
