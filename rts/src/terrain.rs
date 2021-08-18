use crate::{
    assets::terrain::TerrainConfigAsset,
    features::dyn_mesh::{
        DynMeshData, DynMeshDataPart, DynMeshHandle, DynMeshRenderObject, DynMeshRenderObjectSet,
        DynMeshResource,
    },
};
use bevy_tasks::{Task, TaskPool, TaskPoolBuilder};
use building_blocks::{
    core::prelude::*,
    mesh::{
        greedy_quads, padded_greedy_quads_chunk_extent, GreedyQuadsBuffer, IsOpaque, MergeVoxel,
        QuadGroup, RIGHT_HANDED_Y_UP_CONFIG,
    },
    storage::{prelude::*, ChunkHashMap3},
};
use crossbeam_channel::{unbounded, Receiver, Sender};
use glam::{Quat, Vec3};
use legion::{Entity, Resources, World};
use rafx::{
    api::RafxIndexType,
    assets::push_buffer::PushBuffer,
    base::slab::{DropSlab, GenericDropSlabKey},
    rafx_visibility::{
        geometry::{AxisAlignedBoundingBox, BoundingSphere},
        VisibleBounds,
    },
    render_feature_extract_job_predule::*,
    visibility::{CullModel, VisibilityObjectArc},
};
use rafx_plugins::{
    components::{MeshComponent, TransformComponent, VisibilityComponent},
    features::mesh::MeshVertex,
};
use std::{collections::HashMap, sync::Arc};

pub struct ChunkState {
    pub source_version: u32,
    pub rendered_version: u32,
}

pub struct TerrainRenderChunk {
    pub entity: Option<Entity>,
    pub dyn_mesh_handle: Option<DynMeshHandle>,
    pub render_object_handle: Option<RenderObjectHandle>,
    pub visibility_object_handle: Option<VisibilityObjectArc>,
    pub source_version: u32,
    pub rendered_version: u32,
    pub render_task: Option<Task<()>>,
}

impl TerrainRenderChunk {
    pub fn new() -> Self {
        TerrainRenderChunk {
            entity: None,
            dyn_mesh_handle: None,
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
    pub mesh: DynMeshData,
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
    config: TerrainConfigAsset,
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

    pub fn update_render_chunks(&mut self, world: &mut World, resources: &Resources) {
        let mut dyn_mesh_resource = resources.get_mut::<DynMeshResource>().unwrap();
        let mut dyn_mesh_render_objects = resources.get_mut::<DynMeshRenderObjectSet>().unwrap();
        let visibility_region = resources.get::<VisibilityRegion>().unwrap();
        let to_render: Vec<(_, _, Array3x1<CubeVoxel>)> = self
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
            let config = self.config.clone();
            let task = self.task_pool.spawn(async move {
                let mut buffer = GreedyQuadsBuffer::new(
                    padded_chunk_extent,
                    RIGHT_HANDED_Y_UP_CONFIG.quad_groups(),
                );
                greedy_quads(&padded_chunk, &padded_chunk_extent, &mut buffer);
                let extent = padded_chunk_extent.padded(-1);
                let mesh = Self::make_dyn_mesh_data(&padded_chunk, &buffer, extent, &config);
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

        let mut chunks = 0;
        for result in self.render_rx.try_iter() {
            if let Some(chunk) = self.render_chunks.get_mut(&result.key) {
                chunk.render_task = None;
                chunk.rendered_version += 1;
                chunks += 1;

                let visible_bounds = result.mesh.visible_bounds.clone();
                if let Some(handle) = &chunk.dyn_mesh_handle {
                    let _res = dyn_mesh_resource.update_dyn_mesh(&handle, result.mesh);
                } else if let Ok(handle) = dyn_mesh_resource.add_dyn_mesh(result.mesh) {
                    chunk.dyn_mesh_handle = Some(handle.clone());

                    let transform_component = TransformComponent {
                        translation: visible_bounds.aabb.min,
                        scale: Vec3::ONE,
                        rotation: Quat::IDENTITY,
                    };

                    let render_object_handle = dyn_mesh_render_objects
                        .register_render_object(DynMeshRenderObject { mesh: handle });
                    let mesh_component = MeshComponent {
                        render_object_handle: render_object_handle.clone(),
                    };

                    let entity = world.push((transform_component, mesh_component));
                    chunk.entity = Some(entity);

                    let visibility_object_handle = {
                        let handle = visibility_region.register_static_object(
                            ObjectId::from(entity),
                            CullModel::VisibleBounds(visible_bounds),
                        );
                        handle.set_transform(
                            transform_component.translation,
                            transform_component.rotation,
                            transform_component.scale,
                        );
                        handle.add_render_object(&render_object_handle);
                        handle
                    };
                    let mut entry = world.entry(entity).unwrap();
                    entry.add_component(VisibilityComponent {
                        visibility_object_handle: visibility_object_handle.clone(),
                    });

                    chunk.visibility_object_handle = Some(visibility_object_handle);
                    chunk.render_object_handle = Some(render_object_handle);
                }
            };
        }
        if chunks > 0 {
            log::info!("{} terrain meshes generated", chunks,);
        }
    }

    fn make_dyn_mesh_data(
        voxels: &Array3x1<CubeVoxel>,
        quads: &GreedyQuadsBuffer,
        extent: Extent3i,
        config: &TerrainConfigAsset,
    ) -> DynMeshData {
        let mut quad_parts = HashMap::new();
        for (idx, group) in quads.quad_groups.iter().enumerate() {
            for quad in group.quads.iter() {
                let mat = voxels.get(quad.minimum);
                let entry = quad_parts
                    .entry(mat.0 - 1)
                    .or_insert(PerMaterialGreedyQuadsBuffer::new(mat));
                entry.quad_groups[idx].quads.push(*quad);
            }
        }

        let mut all_vertices = PushBuffer::new(16384);
        let mut all_indices = PushBuffer::new(16384);
        let mut mesh_parts: Vec<DynMeshDataPart> = Vec::with_capacity(quad_parts.len());
        for (mat, quads) in quad_parts.iter() {
            let mesh_part = {
                let material_instance = config.inner.materials.get(*mat as usize);
                if let Some(material_instance) = material_instance {
                    let vertex_offset =
                        all_vertices.pad_to_alignment(std::mem::size_of::<MeshVertex>());
                    let indices_offset = all_indices.pad_to_alignment(std::mem::size_of::<u32>());
                    for group in quads.quad_groups.iter() {
                        for quad in group.quads.iter() {
                            let face = &group.face;
                            let vertex_offset =
                                all_vertices.pad_to_alignment(std::mem::size_of::<MeshVertex>());
                            let positions = &face.quad_mesh_positions(quad, 1.0);
                            let normals = &face.quad_mesh_normals();
                            let uvs =
                                face.tex_coords(RIGHT_HANDED_Y_UP_CONFIG.u_flip_face, true, quad);
                            for i in 0..4 {
                                all_vertices.push(
                                    &[MeshVertex {
                                        position: positions[i],
                                        normal: normals[i],
                                        tangent: Default::default(),
                                        tex_coord: uvs[i],
                                    }],
                                    1,
                                );
                            }
                            let indices = &face.quad_mesh_indices(vertex_offset as u32);
                            all_indices.push(indices, std::mem::size_of::<u32>());
                        }
                    }
                    let vertex_size = all_vertices.len() - vertex_offset;
                    let indices_size = all_indices.len() - indices_offset;

                    Some(DynMeshDataPart {
                        material_instance: material_instance.clone(),
                        vertex_buffer_offset_in_bytes: vertex_offset as u32,
                        vertex_buffer_size_in_bytes: vertex_size as u32,
                        index_buffer_offset_in_bytes: indices_offset as u32,
                        index_buffer_size_in_bytes: indices_size as u32,
                        index_type: RafxIndexType::Uint32,
                    })
                } else {
                    log::error!(
                        "Invalid terrain material index {} (# of materials: {})",
                        mat,
                        config.inner.materials.len()
                    );
                    None
                }
            };
            if let Some(mesh_part) = mesh_part {
                mesh_parts.push(mesh_part);
            }
        }

        DynMeshData {
            mesh_parts,
            vertex_buffer: Some(all_vertices.into_data()),
            index_buffer: Some(all_indices.into_data()),
            visible_bounds: Self::make_visible_bounds(&extent, 0),
        }
    }

    fn make_visible_bounds(extent: &Extent3i, hash: u64) -> VisibleBounds {
        let min = extent.minimum;
        let min = Vec3::new(min.x() as f32, min.y() as f32, min.z() as f32);
        let max = extent.max();
        let max = Vec3::new(max.x() as f32, max.y() as f32, max.z() as f32);
        let sphere_center = Vec3::new(
            min.x + (max.x - min.x) / 2.,
            min.y + (max.y - min.y) / 2.,
            min.z + (max.z - min.z) / 2.,
        );
        let sphere_radius = sphere_center.distance(max);

        VisibleBounds {
            aabb: AxisAlignedBoundingBox { min, max },
            obb: Default::default(),
            bounding_sphere: BoundingSphere::new(sphere_center, sphere_radius),
            hash,
        }
    }
}

pub struct PerMaterialGreedyQuadsBuffer {
    pub quad_groups: [QuadGroup; 6],
    pub material: CubeVoxel,
}

impl PerMaterialGreedyQuadsBuffer {
    pub fn new(material: CubeVoxel) -> Self {
        PerMaterialGreedyQuadsBuffer {
            quad_groups: RIGHT_HANDED_Y_UP_CONFIG.quad_groups(),
            material,
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

    pub fn new_terrain(
        &mut self,
        config: TerrainConfigAsset,
        fill_extent: Extent3i,
        fill_value: CubeVoxel,
    ) -> TerrainHandle {
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
                config,
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
