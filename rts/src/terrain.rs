use bevy_tasks::{Task, TaskPool, TaskPoolBuilder};
use building_blocks::{
    core::prelude::*,
    mesh::{
        greedy_quads, padded_greedy_quads_chunk_extent, GreedyQuadsBuffer, IsOpaque, MergeVoxel,
        PosNormTexMesh, RIGHT_HANDED_Y_UP_CONFIG,
    },
    storage::{prelude::*, ChunkHashMap3},
};
use crossbeam_channel::{unbounded, Receiver, Sender};
use rafx::{
    base::slab::{DropSlab, GenericDropSlabKey},
    render_feature_extract_job_predule::*,
    visibility::VisibilityObjectArc,
};
use std::{collections::HashMap, sync::Arc};

pub struct ChunkState {
    pub source_version: u32,
    pub rendered_version: u32,
}

pub struct TerrainRenderChunk {
    pub render_object_handle: Option<RenderObjectHandle>,
    pub visibility_object_handle: Option<VisibilityObjectArc>,
    pub source_version: u32,
    pub rendered_version: u32,
    pub render_task: Option<Task<()>>,
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

pub struct RenderChunkTaskResults {
    pub key: ChunkKey3,
    pub mesh: PosNormTexMesh,
}

#[derive(Clone, Copy, Default)]
pub struct CubeVoxel(u16);

impl From<u16> for CubeVoxel {
    fn from(id: u16) -> Self {
        CubeVoxel(id)
    }
}

impl MergeVoxel for CubeVoxel {
    type VoxelValue = u16;

    fn voxel_merge_value(&self) -> Self::VoxelValue {
        self.0
    }
}

impl IsOpaque for CubeVoxel {
    fn is_opaque(&self) -> bool {
        true
    }
}

impl IsEmpty for CubeVoxel {
    fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

pub struct Terrain {
    pub voxels: ChunkHashMap3<CubeVoxel, ChunkMapBuilder3x1<CubeVoxel>>,
    task_pool: TaskPool,
    render_chunks: HashMap<ChunkKey3, TerrainRenderChunk>,
    render_tx: Sender<RenderChunkTaskResults>,
    render_rx: Receiver<RenderChunkTaskResults>,
}

impl Terrain {
    pub fn set_chunk_dirty(&mut self, chunk: ChunkKey3) -> bool {
        let entry = self
            .render_chunks
            .entry(chunk)
            .or_insert(TerrainRenderChunk::new());
        if entry.source_version == entry.rendered_version {
            entry.source_version += 1;
            false
        } else {
            true
        }
    }

    pub fn reset_chunks(&mut self) {
        self.render_chunks.clear();
        let full_extent = self.voxels.bounding_extent(0);
        let mut occupied = vec![];
        self.voxels.visit_occupied_chunks(0, &full_extent, |chunk| {
            occupied.push(chunk.extent().minimum);
        });
        for chunk_min in occupied {
            self.set_chunk_dirty(ChunkKey3::new(0, chunk_min));
        }
    }

    pub fn update_render_chunks(&mut self) {
        let to_render: Vec<_> = self
            .render_chunks
            .iter()
            .filter(|(_key, chunk)| {
                chunk.render_task.is_none() && chunk.rendered_version < chunk.source_version
            })
            .map(|(key, _chunk)| {
                let padded_chunk_extent = padded_greedy_quads_chunk_extent(
                    &self.voxels.indexer.extent_for_chunk_with_min(key.minimum),
                );
                let mut padded_chunk = Array3x1::fill(padded_chunk_extent, CubeVoxel(0));
                copy_extent(
                    &padded_chunk_extent,
                    &self.voxels.lod_view(0),
                    &mut padded_chunk,
                );
                (key.clone(), padded_chunk_extent, padded_chunk)
            })
            .collect();
        if to_render.len() > 0 {
            log::info!("Starting {} greedy mesh jobs", to_render.len());
        }
        for (key, padded_chunk_extent, padded_chunk) in to_render {
            let render_tx = self.render_tx.clone();
            let task = self.task_pool.spawn(async move {
                let mut buffer = GreedyQuadsBuffer::new(
                    padded_chunk_extent,
                    RIGHT_HANDED_Y_UP_CONFIG.quad_groups(),
                );
                greedy_quads(&padded_chunk, &padded_chunk_extent, &mut buffer);
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
                    key: key.clone(),
                    mesh,
                };
                let _result = render_tx.send(results);
            });
            if let Some(chunk) = self.render_chunks.get_mut(&key) {
                chunk.render_task = Some(task);
            }
        }
        let mut chunks = vec![];
        for result in self.render_rx.try_iter() {
            if let Some(chunk) = self.render_chunks.get_mut(&result.key) {
                chunk.render_task = None;
                chunk.rendered_version += 1;

                chunks.push((
                    result.key.minimum,
                    result.mesh.positions.len(),
                    result.mesh.indices.len(),
                ));
            };
        }
        if chunks.len() > 0 {
            let tot_pos: usize = chunks.iter().map(|p| p.1).sum();
            let tot_ind: usize = chunks.iter().map(|p| p.2).sum();
            log::info!(
                "{} terrain meshes generated: {} positions, {} indices",
                chunks.len(),
                tot_pos,
                tot_ind,
            );
        }
    }
}

#[derive(Clone, Debug)]
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
        let storage = &self.storage;
        storage.try_read().unwrap_or_else(move || {
            log::warn!("TerrainStorage is being written by another thread.");

            storage.read()
        })
    }

    pub fn write(&mut self) -> RwLockWriteGuard<TerrainStorage> {
        let storage = &self.storage;
        storage.try_write().unwrap_or_else(move || {
            log::warn!("TerrainStorage is being read or written by another thread.");

            storage.write()
        })
    }

    pub fn new_terrain(&mut self, fill_extent: Extent3i, fill_value: CubeVoxel) -> TerrainHandle {
        let mut terrain = {
            let voxels = {
                let chunk_shape = Point3i::fill(16);
                let ambient_value = CubeVoxel::default();
                let builder = ChunkMapBuilder3x1::new(chunk_shape, ambient_value);
                let mut voxels = builder.build_with_hash_map_storage();
                let mut lod0 = voxels.lod_view_mut(0);
                lod0.fill_extent(&fill_extent, fill_value);
                voxels
            };

            let (render_tx, render_rx) = unbounded();
            Terrain {
                voxels,
                task_pool: TaskPoolBuilder::new().build(),
                render_chunks: HashMap::new(),
                render_tx,
                render_rx,
            }
        };

        terrain.reset_chunks();

        let terrain_handle = {
            let mut storage = self.write();
            storage.register_terrain(terrain)
        };

        terrain_handle
    }
}

#[derive(Clone)]
pub struct TerrainComponent {
    pub handle: TerrainHandle,
}
