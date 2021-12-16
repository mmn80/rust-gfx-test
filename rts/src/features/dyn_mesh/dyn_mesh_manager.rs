use std::{ops::Deref, sync::Arc};

use crossbeam_channel::{Receiver, Sender};
use fnv::FnvHashMap;
use rafx::{
    api::{RafxBuffer, RafxDeviceContext, RafxError, RafxQueue, RafxResourceType},
    assets::AssetManager,
    base::{
        memory::force_to_static_lifetime,
        slab::{DropSlab, GenericDropSlabKey},
    },
    RafxResult,
};

pub use super::buffer_upload::BufferUploaderConfig;
use super::{
    buffer_upload::{BufferUploadId, BufferUploadResult, BufferUploader},
    DynMesh, DynMeshData, DynMeshInner, DynMeshPart,
};

struct DynMeshUpload {
    pub mesh_data: DynMeshData,
    pub vertex_full_upload_id: BufferUploadId,
    pub vertex_full_rx: Receiver<BufferUploadResult>,
    pub vertex_full_buffer: Option<RafxBuffer>,
    pub vertex_full_buffer_uploaded: bool,
    pub vertex_position_upload_id: BufferUploadId,
    pub vertex_position_rx: Receiver<BufferUploadResult>,
    pub vertex_position_buffer: Option<RafxBuffer>,
    pub vertex_position_buffer_uploaded: bool,
    pub index_upload_id: BufferUploadId,
    pub index_rx: Receiver<BufferUploadResult>,
    pub index_buffer: Option<RafxBuffer>,
    pub index_buffer_uploaded: bool,
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

impl std::fmt::Display for DynMeshHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.key)
    }
}

pub enum DynMeshCommand {
    Add {
        request_handle: usize,
        data: DynMeshData,
    },
    Update {
        request_handle: usize,
        handle: DynMeshHandle,
        data: DynMeshData,
    },
}

pub enum DynMeshCommandResults {
    Add {
        request_handle: usize,
        result: RafxResult<DynMeshHandle>,
    },
    Update {
        request_handle: usize,
        result: RafxResult<()>,
    },
}

pub struct DynMeshManager {
    storage: DropSlab<DynMeshState>,
    cmd_in_tx: Sender<DynMeshCommand>,
    cmd_in_rx: Receiver<DynMeshCommand>,
    cmd_out_tx: Sender<DynMeshCommandResults>,
    cmd_out_rx: Receiver<DynMeshCommandResults>,
    uploader: Option<BufferUploader>,
    vertex_full_uploads: FnvHashMap<BufferUploadId, DynMeshHandle>,
    vertex_full_tx: Sender<BufferUploadResult>,
    vertex_full_rx: Receiver<BufferUploadResult>,
    vertex_position_uploads: FnvHashMap<BufferUploadId, DynMeshHandle>,
    vertex_position_tx: Sender<BufferUploadResult>,
    vertex_position_rx: Receiver<BufferUploadResult>,
    index_uploads: FnvHashMap<BufferUploadId, DynMeshHandle>,
    index_tx: Sender<BufferUploadResult>,
    index_rx: Receiver<BufferUploadResult>,
}

impl DynMeshManager {
    pub fn new() -> Self {
        let (cmd_in_tx, cmd_in_rx) = crossbeam_channel::unbounded();
        let (cmd_out_tx, cmd_out_rx) = crossbeam_channel::unbounded();
        let (vertex_full_tx, vertex_full_rx) = crossbeam_channel::unbounded();
        let (vertex_position_tx, vertex_position_rx) = crossbeam_channel::unbounded();
        let (index_tx, index_rx) = crossbeam_channel::unbounded();
        Self {
            storage: Default::default(),
            cmd_in_tx,
            cmd_in_rx,
            cmd_out_tx,
            cmd_out_rx,
            uploader: None,
            vertex_full_uploads: Default::default(),
            vertex_full_tx,
            vertex_full_rx,
            vertex_position_uploads: Default::default(),
            vertex_position_tx,
            vertex_position_rx,
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
            match BufferUploader::new(
                device_context,
                upload_queue_config,
                graphics_queue,
                transfer_queue,
            ) {
                Ok(uploader) => self.uploader = Some(uploader),
                Err(err) => log::error!("BufferUploadManager: {}", err),
            }
        } else {
            log::error!("BufferUploadManager already initialized");
        }
    }

    pub fn get_command_channels(
        &self,
    ) -> (Sender<DynMeshCommand>, Receiver<DynMeshCommandResults>) {
        (self.cmd_in_tx.clone(), self.cmd_out_rx.clone())
    }

    #[profiling::function]
    fn start_upload(
        &mut self,
        mut mesh_data: DynMeshData,
        handle: Option<&DynMeshHandle>,
    ) -> RafxResult<DynMeshState> {
        if mesh_data.vertex_full_buffer.is_none()
            || mesh_data.vertex_position_buffer.is_none()
            || mesh_data.index_buffer.is_none()
        {
            return Err(RafxError::StringError(
                "Dyn mesh data is not initialized".to_string(),
            ));
        }
        let vertex_full_data = std::mem::take(&mut mesh_data.vertex_full_buffer).unwrap();
        let vertex_position_data = std::mem::take(&mut mesh_data.vertex_position_buffer).unwrap();
        let index_data = std::mem::take(&mut mesh_data.index_buffer).unwrap();

        if vertex_full_data.is_empty() || vertex_position_data.is_empty() || index_data.is_empty() {
            return Err(RafxError::StringError(
                "Dyn mesh data does not contain data".to_string(),
            ));
        }

        let uploader = self.uploader.as_ref().unwrap();
        let vertex_full_upload_id = uploader.upload_buffer(
            RafxResourceType::VERTEX_BUFFER,
            vertex_full_data,
            self.vertex_full_tx.clone(),
        )?;
        let vertex_position_upload_id = uploader.upload_buffer(
            RafxResourceType::VERTEX_BUFFER,
            vertex_position_data,
            self.vertex_position_tx.clone(),
        )?;
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
                vertex_full_upload_id,
                vertex_full_rx: self.vertex_full_rx.clone(),
                vertex_full_buffer: None,
                vertex_full_buffer_uploaded: false,
                vertex_position_upload_id,
                vertex_position_rx: self.vertex_position_rx.clone(),
                vertex_position_buffer: None,
                vertex_position_buffer_uploaded: false,
                index_upload_id,
                index_rx: self.index_rx.clone(),
                index_buffer: None,
                index_buffer_uploaded: false,
            },
            old_dyn_mash,
        ))
    }

    #[profiling::function]
    fn process_upload_results(&mut self, asset_manager: &mut AssetManager) {
        for upload_result in self.vertex_full_rx.try_iter().collect::<Vec<_>>() {
            let (upload_id, buffer) = match upload_result {
                BufferUploadResult::UploadError(upload_id) => (upload_id, None),
                BufferUploadResult::UploadDrop(upload_id) => (upload_id, None),
                BufferUploadResult::UploadComplete(upload_id, buffer) => (upload_id, Some(buffer)),
            };
            let handle = self.vertex_full_uploads.get(&upload_id).unwrap().clone();
            if let (Some(buffer), DynMeshState::Uploading(ref mut upload, _)) =
                (buffer, self.get_mut(&handle))
            {
                upload.vertex_full_buffer = Some(buffer);
                upload.vertex_full_buffer_uploaded = true;
            } else {
                log::error!(
                    "Vertex buffer upload error (upload id: {:?}) for dyn mesh: {:?}",
                    upload_id,
                    handle
                );
                let _old = std::mem::replace(self.get_mut(&handle), DynMeshState::UploadError);
            }
            self.vertex_full_uploads.remove(&upload_id);
            self.check_finished_upload(&handle, asset_manager);
        }
        for upload_result in self.vertex_position_rx.try_iter().collect::<Vec<_>>() {
            let (upload_id, buffer) = match upload_result {
                BufferUploadResult::UploadError(upload_id) => (upload_id, None),
                BufferUploadResult::UploadDrop(upload_id) => (upload_id, None),
                BufferUploadResult::UploadComplete(upload_id, buffer) => (upload_id, Some(buffer)),
            };
            let handle = self
                .vertex_position_uploads
                .get(&upload_id)
                .unwrap()
                .clone();
            if let (Some(buffer), DynMeshState::Uploading(ref mut upload, _)) =
                (buffer, self.get_mut(&handle))
            {
                upload.vertex_position_buffer = Some(buffer);
                upload.vertex_position_buffer_uploaded = true;
            } else {
                log::error!(
                    "Vertex buffer upload error (upload id: {:?}) for dyn mesh: {:?}",
                    upload_id,
                    handle
                );
                let _old = std::mem::replace(self.get_mut(&handle), DynMeshState::UploadError);
            }
            self.vertex_position_uploads.remove(&upload_id);
            self.check_finished_upload(&handle, asset_manager);
        }
        for upload_result in self.index_rx.try_iter().collect::<Vec<_>>() {
            let (upload_id, buffer) = match upload_result {
                BufferUploadResult::UploadError(upload_id) => (upload_id, None),
                BufferUploadResult::UploadDrop(upload_id) => (upload_id, None),
                BufferUploadResult::UploadComplete(upload_id, buffer) => (upload_id, Some(buffer)),
            };
            let handle = self.index_uploads.get(&upload_id).unwrap().clone();
            if let (Some(buffer), DynMeshState::Uploading(ref mut upload, _)) =
                (buffer, self.get_mut(&handle))
            {
                upload.index_buffer = Some(buffer);
                upload.index_buffer_uploaded = true;
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
            if !upload.vertex_full_buffer_uploaded
                || !upload.vertex_position_buffer_uploaded
                || !upload.index_buffer_uploaded
            {
                return;
            }
            if let (Some(vertex_full_buffer), Some(vertex_position_buffer), Some(index_buffer)) = (
                upload.vertex_full_buffer.take(),
                upload.vertex_position_buffer.take(),
                upload.index_buffer.take(),
            ) {
                let visible_bounds = upload.mesh_data.visible_bounds;
                let vertex_full_buffer =
                    asset_manager.resources().insert_buffer(vertex_full_buffer);
                let vertex_position_buffer = asset_manager
                    .resources()
                    .insert_buffer(vertex_position_buffer);
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
                            vertex_full_buffer_offset_in_bytes: mesh_part
                                .vertex_full_buffer_offset_in_bytes,
                            vertex_full_buffer_size_in_bytes: mesh_part
                                .vertex_full_buffer_size_in_bytes,
                            vertex_position_buffer_offset_in_bytes: mesh_part
                                .vertex_position_buffer_offset_in_bytes,
                            vertex_position_buffer_size_in_bytes: mesh_part
                                .vertex_position_buffer_size_in_bytes,
                            index_buffer_offset_in_bytes: mesh_part.index_buffer_offset_in_bytes,
                            index_buffer_size_in_bytes: mesh_part.index_buffer_size_in_bytes,
                            index_type: mesh_part.index_type,
                        })
                    })
                    .collect();

                let inner = DynMeshInner {
                    vertex_full_buffer,
                    vertex_position_buffer,
                    index_buffer,
                    mesh_parts,
                    visible_bounds,
                };
                let dyn_mesh = DynMesh {
                    inner: Arc::new(inner),
                };

                let _old = std::mem::replace(mesh_state, DynMeshState::Completed(dyn_mesh));
            } else {
                unreachable!();
            }
        }
    }

    #[profiling::function]
    fn add_dyn_mesh(&mut self, mesh_data: DynMeshData) -> RafxResult<DynMeshHandle> {
        let mesh_state = self.start_upload(mesh_data, None)?;

        self.storage.process_drops();
        let drop_slab_key = self.storage.allocate(mesh_state);
        let handle = DynMeshHandle {
            key: drop_slab_key.generic_drop_slab_key(),
        };

        let mesh_state = self.storage.get(&drop_slab_key).unwrap();
        if let DynMeshState::Uploading(upload, _) = mesh_state {
            self.vertex_full_uploads
                .insert(upload.vertex_full_upload_id.clone(), handle.clone());
            self.vertex_position_uploads
                .insert(upload.vertex_position_upload_id.clone(), handle.clone());
            self.index_uploads
                .insert(upload.index_upload_id.clone(), handle.clone());
        } else {
            unreachable!();
        }

        Ok(handle)
    }

    fn get(&self, handle: &DynMeshHandle) -> &DynMeshState {
        self.storage
            .get(&handle.key.drop_slab_key())
            .unwrap_or_else(|| panic!("DynMeshStorage did not contain handle {:?}.", handle))
    }

    fn get_mut(&mut self, handle: &DynMeshHandle) -> &mut DynMeshState {
        self.storage
            .get_mut(&handle.key.drop_slab_key())
            .unwrap_or_else(|| panic!("DynMeshStorage did not contain handle {:?}.", handle))
    }

    pub fn get_dyn_mesh(&self, handle: &DynMeshHandle) -> Option<DynMesh> {
        match self.get(handle) {
            DynMeshState::Uploading(_, old_dyn_mesh) => old_dyn_mesh.clone(),
            DynMeshState::Completed(mesh) => Some(mesh.clone()),
            DynMeshState::UploadError => None,
        }
    }

    #[profiling::function]
    pub fn update(&mut self, asset_manager: &mut AssetManager) {
        if let Some(ref mut upload) = self.uploader {
            let _res = upload.update();
        }
        self.process_upload_results(asset_manager);

        let mut commands = vec![];
        for cmd in self.cmd_in_rx.try_iter() {
            commands.push(cmd);
        }
        for cmd in commands {
            match cmd {
                DynMeshCommand::Add {
                    request_handle,
                    data,
                } => {
                    let result = self.add_dyn_mesh(data);
                    let _res = self.cmd_out_tx.send(DynMeshCommandResults::Add {
                        request_handle,
                        result,
                    });
                }
                DynMeshCommand::Update {
                    request_handle,
                    handle,
                    data,
                } => {
                    let result = match self.start_upload(data, Some(&handle)) {
                        Ok(mesh_state) => {
                            if let DynMeshState::Uploading(ref upload, _) = mesh_state {
                                self.vertex_full_uploads
                                    .insert(upload.vertex_full_upload_id.clone(), handle.clone());
                                self.vertex_position_uploads.insert(
                                    upload.vertex_position_upload_id.clone(),
                                    handle.clone(),
                                );
                                self.index_uploads
                                    .insert(upload.index_upload_id.clone(), handle.clone());
                            } else {
                                unreachable!();
                            }

                            let old_mesh_state = self.get_mut(&handle);
                            let _old = std::mem::replace(old_mesh_state, mesh_state);
                            Ok(())
                        }
                        Err(err) => Err(err),
                    };
                    let _res = self.cmd_out_tx.send(DynMeshCommandResults::Update {
                        request_handle,
                        result,
                    });
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct DynMeshManagerExtractRef(Arc<&'static DynMeshManager>);

impl DynMeshManagerExtractRef {
    // Cannot use begin_extract / end_extract like for AssetManager because it is hardcoded into Renderer.
    // This should exclusively be created & destroyed as part of extract jobs.
    pub unsafe fn new(dyn_mesh_manager: &DynMeshManager) -> Self {
        Self(Arc::new(force_to_static_lifetime(dyn_mesh_manager)))
    }
}

impl Deref for DynMeshManagerExtractRef {
    type Target = DynMeshManager;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}
