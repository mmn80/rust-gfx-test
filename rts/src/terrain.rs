use building_blocks::{
    core::prelude::*,
    storage::{prelude::*, ChunkHashMap3},
};
use rafx::{
    base::slab::{DropSlab, GenericDropSlabKey},
    render_feature_extract_job_predule::{RenderObjectHandle, RwLock},
    visibility::VisibilityObjectArc,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct TerrainRenderChunk {
    pub extent: Extent3i,
    pub render_object_handle: RenderObjectHandle,
    pub visibility_object_handle: VisibilityObjectArc,
}

pub struct TerrainInner {
    pub voxels: ChunkHashMap3<u16, ChunkMapBuilder3x1<u16>>,
    pub render_chunks: Vec<TerrainRenderChunk>,
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

            Terrain {
                inner: Arc::new(TerrainInner {
                    voxels,
                    render_chunks: vec![],
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
