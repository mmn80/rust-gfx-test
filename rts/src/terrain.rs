use bevy_tasks::{Task, TaskPool, TaskPoolBuilder};
use building_blocks::{
    core::prelude::*,
    mesh::{
        greedy_quads, padded_greedy_quads_chunk_extent, GreedyQuadsBuffer, PosNormTexMesh,
        RIGHT_HANDED_Y_UP_CONFIG,
    },
    storage::{prelude::*, ChunkHashMap3},
};
use crossbeam_channel::{unbounded, Receiver, Sender};
use rafx::{
    base::slab::{DropSlab, GenericDropSlabKey},
    render_feature_extract_job_predule::{RenderObjectHandle, RwLock},
    visibility::VisibilityObjectArc,
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

pub struct ChunkState {
    pub source_version: u32,
    pub rendered_version: u32,
}

#[derive(Clone)]
pub struct TerrainRenderChunk {
    pub render_object_handle: Option<RenderObjectHandle>,
    pub visibility_object_handle: Option<VisibilityObjectArc>,
    pub source_version: u32,
    pub rendered_version: u32,
    pub render_task: Option<Task<bool>>,
}

impl TerrainRenderChunk {
    pub fn new() -> Self {
        TerrainRenderChunk {
            render_object_handle: None,
            visibility_object_handle: None,
            source_version: 0,
            rendered_version: 0,
            render_task: None,
        }
    }
}

#[derive(Sync, Send)]
pub struct RenderChunkTaskResults {
    pub key: ChunkKey3,
    pub mesh: PosNormTexMesh,
}

pub struct TerrainInner {
    pub voxels: ChunkHashMap3<u16, ChunkMapBuilder3x1<u16>>,
    task_pool: TaskPool,
    render_chunks: HashMap<ChunkKey3, TerrainRenderChunk>,
    render_tx: Sender<RenderChunkTaskResults>,
    render_rx: Receiver<RenderChunkTaskResults>,
}

impl TerrainInner {
    pub fn set_chunk_dirty(&mut self, chunk: ChunkKey3) -> bool {
        let entry = self
            .render_chunks
            .entry(chunk)
            .or_insert(TerrainRenderChunk::new());
        entry.source_version += 1;
    }

    pub fn reset_chunks(&mut self) {
        self.render_chunks.clear();
        let full_extent = self.voxels.bounding_extent(0);
        self.voxels.visit_occupied_chunks(0, full_extent, |chunk| {
            self.set_chunk_dirty(chunk.extent().into());
        });
    }

    pub fn update_render_chunks(&mut self) {
        for (key, chunk) in self.render_chunks.iter_mut().filter(|(_key, chunk)| {
            chunk.render_task.is_none() && chunk.rendered_version < chunk.source_version
        }) {
            chunk.render_task = {
                let render_tx = self.render_tx.clone();
                let padded_chunk_extent = padded_greedy_quads_chunk_extent(
                    self.voxels.indexer.extent_for_chunk_with_min(key.minimum),
                );
                let mut padded_chunk = Array3x1::fill(padded_chunk_extent, 0);
                copy_extent(
                    &padded_chunk_extent,
                    &self.voxels.lod_view(0),
                    &mut padded_chunk,
                );
                let padded_chunk_ref = &padded_chunk;
                let task = self.task_pool.spawn(async move {
                    let mut buffer = GreedyQuadsBuffer::new(
                        padded_chunk_extent,
                        RIGHT_HANDED_Y_UP_CONFIG.quad_groups(),
                    );
                    greedy_quads(padded_chunk_ref, &padded_chunk_extent, &mut buffer);
                    let mut mesh = PosNormTexMesh::default();
                    for group in buffer.quad_groups.iter() {
                        for quad in group.quads.iter() {
                            group.face.add_quad_to_pos_norm_tex_mesh(
                                RIGHT_HANDED_Y_UP_CONFIG.u_flip_face,
                                false,
                                &quad,
                                1.0,
                                &mut mesh,
                            );
                        }
                    }
                    let results = RenderChunkTaskResults {
                        key,
                        mesh: Some(mesh),
                    };
                    render_tx.send(results);
                });
                task.detach();
                Some(task)
            }
        }
        for result in self.render_rx.try_iter() {
            if let Some(chunk) = self.render_chunks.get_mut(result.key) {
                chunk.render_task = None;
                chunk.rendered_version += 1;

                log::info!(
                    "Greedy mesh {} generated: {} positions, {} indices",
                    result.key,
                    result.mesh.positions.len(),
                    result.mesh.indices.len()
                );
            };
        }
    }
}

#[derive(Clone)]
pub struct Terrain {
    pub inner: Arc<TerrainInner>,
}

#[derive(Copy, Eq, PartialEq, Hash, Clone, Debug)]
pub struct TerrainHandle {
    handle: GenericDropSlabKey,
}

pub struct TerrainStorage {
    inner: DropSlab<Terrain>,
}

impl TerrainStorage {
    pub fn new() -> Self {
        Self {
            inner: Default::default(),
        }
    }

    pub fn register_terrain(&mut self, terrain: Terrain) -> TerrainHandle {
        self.inner.process_drops();

        let drop_slab_key = self.inner.allocate(terrain);
        TerrainHandle {
            handle: drop_slab_key.generic_drop_slab_key(),
        }
    }

    pub fn get(&self, terrain_handle: &TerrainHandle) -> &Terrain {
        self.inner
            .get(&terrain_handle.handle.drop_slab_key())
            .unwrap_or_else(|| {
                panic!(
                    "TerrainStorage did not contain handle {:?}.",
                    terrain_handle
                )
            })
    }

    pub fn get_mut(&mut self, terrain_handle: &TerrainHandle) -> &mut Terrain {
        self.inner
            .get_mut(&terrain_handle.handle.drop_slab_key())
            .unwrap_or_else(|| {
                panic!(
                    "TerrainStorage did not contain handle {:?}.",
                    terrain_handle
                )
            })
    }
}

#[derive(Clone)]
pub struct TerrainResource {
    storage: Arc<RwLock<TerrainStorage>>,
}

impl TerrainResource {
    pub fn new() -> Self {
        Self {
            storage: Arc::new(RwLock::new(TerrainStorage::new())),
        }
    }

    pub fn read(&self) -> RwLockReadGuard<TerrainStorage> {
        let registry = &self.storage;
        registry.try_read().unwrap_or_else(move || {
            log::warn!("TerrainStorage is being written by another thread.");

            registry.read()
        })
    }

    fn write(&mut self) -> RwLockWriteGuard<TerrainStorage> {
        let registry = &self.storage;
        registry.try_write().unwrap_or_else(move || {
            log::warn!("TerrainStorage is being read or written by another thread.");

            registry.write()
        })
    }

    pub fn new_terrain(&mut self, fill_extent: Extent3i, fill_value: u16) -> TerrainHandle {
        let terrain = {
            let voxels = {
                let chunk_shape = Point3i::fill(16);
                let ambient_value = 0;
                let builder = ChunkMapBuilder3x1::new(chunk_shape, ambient_value);
                let mut voxels = builder.build_with_hash_map_storage();
                let mut lod0 = voxels.lod_view_mut(0);
                lod0.fill_extent(fill_extent, fill_value);
                voxels
            };

            let (render_tx, render_rx) = unbounded();
            Terrain {
                inner: Arc::new(TerrainInner {
                    voxels,
                    task_pool: TaskPoolBuilder::new().build(),
                    render_chunks: HashMap::new(),
                    render_tx,
                    render_rx,
                }),
            }
        };
        let terrain_handle = {
            let mut storage = self.write();
            storage.register_terrain(terrain)
        };

        terrain_handle
    }
}
