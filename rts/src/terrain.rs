use std::{
    cmp::{max, min},
    collections::HashMap,
    sync::Arc,
};

use bevy_tasks::{Task, TaskPool, TaskPoolBuilder};
use building_blocks::{
    core::prelude::*,
    mesh::{
        greedy_quads, padded_greedy_quads_chunk_extent, GreedyQuadsBuffer, IsOpaque, MergeVoxel,
        QuadGroup, RIGHT_HANDED_Y_UP_CONFIG,
    },
    search::GridRayTraversal3,
    storage::{prelude::*, ChunkHashMap3},
};
use crossbeam_channel::{unbounded, Receiver, Sender};
use fnv::FnvHashMap;
use glam::{Quat, Vec2, Vec3};
use legion::{Entity, Resources, World};
use rafx::{
    api::RafxIndexType,
    assets::push_buffer::PushBuffer,
    base::{
        slab::{DropSlab, GenericDropSlabKey},
        Instant,
    },
    rafx_visibility::{
        geometry::{AxisAlignedBoundingBox, BoundingSphere},
        VisibleBounds,
    },
    render_features::{
        render_features_prelude::{RwLock, RwLockReadGuard, RwLockWriteGuard},
        RenderObjectHandle,
    },
    renderer::ViewportsResource,
    visibility::{CullModel, ObjectId, VisibilityObjectArc, VisibilityRegion},
};
use rafx_plugins::{
    components::{MeshComponent, TransformComponent, VisibilityComponent},
    features::mesh::MeshVertex,
};

use crate::{
    assets::pbr_material::PbrMaterialAsset,
    features::dyn_mesh::{
        DynMeshData, DynMeshDataPart, DynMeshHandle, DynMeshRenderObject, DynMeshRenderObjectSet,
        DynMeshResource,
    },
    perlin::PerlinNoise2D,
};

pub struct RenderChunkTaskMetrics {
    pub quads_time: u32,   // µs
    pub mesh_time: u32,    // µs
    pub results_time: u32, // µs
    pub failed: bool,
}

pub struct RenderChunkExtractMetrics {
    pub tasks: u32,
    pub extract_time: u32, // µs
}

pub struct SingleDistributionMetrics {
    pub samples: usize,
    pub failed: usize,
    pub min_time: f64, // µs
    pub max_time: f64, // µs
    pub avg_time: f64, // µs
    pub std_dev: f64,
}

impl SingleDistributionMetrics {
    pub fn new(samples: Vec<Option<usize>>) -> Self {
        let total_samples = samples.len();
        let samples: Vec<_> = samples
            .iter()
            .filter_map(|m| m.as_ref())
            .map(|v| *v)
            .collect();
        let mut result = Self {
            samples: total_samples,
            failed: total_samples - samples.len(),
            min_time: *samples.iter().min().unwrap_or(&0) as f64,
            max_time: *samples.iter().max().unwrap_or(&0) as f64,
            avg_time: samples.iter().sum::<usize>() as f64 / samples.len() as f64,
            std_dev: 0.,
        };
        result.std_dev = (samples
            .iter()
            .map(|t| (result.avg_time - *t as f64).powi(2))
            .sum::<f64>()
            / samples.len() as f64)
            .sqrt();
        result
    }

    pub fn info_log(&self, name: &str) {
        log::info!(
            "metrics.{:7} :: samples: {:5}, failed: {}, min: {:2} µs, max: {:4} µs, avg: {:4} µs, std_dev: {:.4}",
            name,
            self.samples,
            self.failed,
            self.min_time as usize,
            self.max_time as usize,
            self.avg_time as usize,
            self.std_dev
        );
    }
}

pub struct RenderChunkDistributionMetrics {
    pub extract_time: SingleDistributionMetrics,
    pub quads_time: SingleDistributionMetrics,
    pub mesh_time: SingleDistributionMetrics,
    pub results_time: SingleDistributionMetrics,
}

impl RenderChunkDistributionMetrics {
    pub fn info_log(&self) {
        self.extract_time.info_log("extract");
        self.quads_time.info_log("quads");
        self.mesh_time.info_log("mesh");
        self.results_time.info_log("results");
    }
}

pub struct RenderChunkMetrics {
    pub start: Instant,
    pub tasks: Vec<RenderChunkTaskMetrics>,
    pub extract: Vec<RenderChunkExtractMetrics>,
}

impl Default for RenderChunkMetrics {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            tasks: Default::default(),
            extract: Default::default(),
        }
    }
}

impl RenderChunkMetrics {
    pub fn is_empty(&self) -> bool {
        self.extract.is_empty() && self.tasks.is_empty()
    }

    pub fn get_distribution_metrics(&self) -> RenderChunkDistributionMetrics {
        let extract_total = self.extract.iter().map(|m| m.tasks as usize).sum();
        let extract_time = SingleDistributionMetrics {
            samples: extract_total,
            failed: 0,
            min_time: 0.,
            max_time: 0.,
            avg_time: self
                .extract
                .iter()
                .map(|r| r.extract_time as usize)
                .sum::<usize>() as f64
                / extract_total as f64,
            std_dev: 0.,
        };

        fn check(failed: bool, value: usize) -> Option<usize> {
            if failed {
                None
            } else {
                Some(value)
            }
        }
        let quads_time = SingleDistributionMetrics::new(
            self.tasks
                .iter()
                .map(|t| check(t.failed, t.quads_time as usize))
                .collect(),
        );
        let mesh_time = SingleDistributionMetrics::new(
            self.tasks
                .iter()
                .map(|t| check(t.failed, t.mesh_time as usize))
                .collect(),
        );
        let results_time = SingleDistributionMetrics::new(
            self.tasks
                .iter()
                .map(|t| check(t.failed, t.results_time as usize))
                .collect(),
        );

        RenderChunkDistributionMetrics {
            extract_time,
            quads_time,
            mesh_time,
            results_time,
        }
    }
}

pub struct RenderChunkTaskResults {
    pub key: ChunkKey3,
    pub mesh: Option<DynMeshData>,
    pub metrics: RenderChunkTaskMetrics,
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

    fn clear(&mut self, world: &mut World) {
        self.dyn_mesh_handle.take();
        self.render_object_handle.take();
        self.visibility_object_handle.take();
        if let Some(entity) = self.entity.take() {
            world.remove(entity);
        }
    }
}

pub type TerrainVoxels = ChunkHashMap3<CubeVoxel, ChunkMapBuilder3x1<CubeVoxel>>;

pub struct Terrain {
    materials: Vec<PbrMaterialAsset>,
    material_names: HashMap<String, u16>,
    pub voxels: TerrainVoxels,
    task_pool: TaskPool,
    render_chunks: HashMap<ChunkKey3, TerrainRenderChunk>,
    render_tx: Sender<RenderChunkTaskResults>,
    render_rx: Receiver<RenderChunkTaskResults>,
    metrics: RenderChunkMetrics,
    initialized: bool,
}

const MAX_RENDER_CHUNK_JOBS: usize = 16;
const MAX_NEW_RENDER_CHUNK_JOBS_PER_FRAME: usize = 4;
const MAX_RENDER_CHUNK_JOBS_INIT: usize = 65536;
const MAX_DISTANCE_FROM_CAMERA: i32 = 256;

impl Terrain {
    pub fn get_default_material_names() -> Vec<&'static str> {
        vec![
            "flat_red",
            "flat_green",
            "flat_blue",
            "metal",
            "round-pattern-wallpaper",
            "diamond-inlay-tile",
            "curly_tile",
            "simple_tile",
            "black_plastic",
        ]
    }

    pub fn material_by_name(&self, name: &'static str) -> Option<PbrMaterialAsset> {
        self.material_names
            .get(name)
            .and_then(|idx| Some(self.materials[*idx as usize].clone()))
    }

    pub fn voxel_by_material(&self, material_name: &'static str) -> Option<CubeVoxel> {
        self.material_names
            .get(material_name)
            .and_then(|idx| Some(CubeVoxel(*idx + 1)))
    }

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

    pub fn update_voxel(&mut self, point: Point3i, voxel: CubeVoxel) {
        let vox_ref: &mut CubeVoxel = self.voxels.get_mut_point(0, point);
        *vox_ref = voxel;
        let keys = self
            .voxels
            .indexer
            .chunk_mins_for_extent(&Extent3i::from_min_and_shape(point, Point3i::ONES).padded(1))
            .map(|p| ChunkKey3::new(0, p));
        for key in keys {
            self.set_chunk_dirty(key);
        }
    }

    pub fn clear_voxel(&mut self, point: Point3i) {
        self.update_voxel(point, 0.into());
    }

    pub fn reset_chunks(&mut self, world: &mut World) {
        for chunk in self.render_chunks.values_mut() {
            chunk.clear(world);
        }
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

    pub fn generate_voxels(
        materials: &HashMap<String, u16>,
        origin: Point3i,
        size: u32,
        style: TerrainFillStyle,
    ) -> TerrainVoxels {
        let chunk_shape = Point3i::fill(16);
        let ambient_value = CubeVoxel::default();
        let builder = ChunkMapBuilder3x1::new(chunk_shape, ambient_value);
        let mut voxels = builder.build_with_hash_map_storage();
        let mut lod0 = voxels.lod_view_mut(0);
        let size = size as i32;
        let base_min = PointN([origin.x() - size / 2, origin.y() - size / 2, origin.z()]);
        let base_extent = Extent3i::from_min_and_shape(base_min, PointN([size, size, 1]));
        match style {
            TerrainFillStyle::FlatBoard { material } => {
                let voxel = CubeVoxel(materials[material] + 1);
                lod0.fill_extent(&base_extent, voxel);
            }
            TerrainFillStyle::CheckersBoard { zero, one } => {
                let zero_voxel = CubeVoxel(materials[zero] + 1);
                let one_voxel = CubeVoxel(materials[one] + 1);
                for p in base_extent.iter_points() {
                    let px = p.x() % 2;
                    let py = p.y() % 2;
                    lod0.fill_extent(
                        &Extent3i::from_min_and_shape(p, Point3i::ONES),
                        if (px + py) % 2 == 0 {
                            zero_voxel
                        } else {
                            one_voxel
                        },
                    );
                }
            }
            TerrainFillStyle::PerlinNoise { params, material } => {
                let voxel = CubeVoxel(materials[material] + 1);
                for p in base_extent.iter_points() {
                    let noise = params.get_noise(p.x() as f64, p.y() as f64) as i32;
                    let top = PointN([p.x(), p.y(), noise - 8]);
                    lod0.fill_extent(&Extent3i::from_min_and_shape(top, PointN([1, 1, 8])), voxel);
                }
            }
        };
        voxels
    }

    pub fn reset(
        &mut self,
        world: &mut World,
        origin: Point3i,
        size: u32,
        style: TerrainFillStyle,
    ) {
        log::info!("Resetting terrain...");

        self.voxels = Self::generate_voxels(&self.material_names, origin, size, style);
        self.reset_chunks(world);

        log::info!("Terrain reset");
    }

    pub fn update_render_chunks(&mut self, world: &mut World, resources: &Resources) {
        let viewports_resource = resources.get::<ViewportsResource>().unwrap();
        let eye = viewports_resource
            .main_view_meta
            .as_ref()
            .and_then(|view| Some(view.eye_position))
            .unwrap_or_default();
        let mut dyn_mesh_resource = resources.get_mut::<DynMeshResource>().unwrap();
        let mut dyn_mesh_render_objects = resources.get_mut::<DynMeshRenderObjectSet>().unwrap();
        let visibility_region = resources.get::<VisibilityRegion>().unwrap();

        let current_jobs = self
            .render_chunks
            .iter()
            .filter(|(_key, chunk)| chunk.render_task.is_some())
            .count();
        if !self.initialized || current_jobs < MAX_RENDER_CHUNK_JOBS {
            let extract_start = Instant::now();
            let mut changed_keys: Vec<_> = self
                .render_chunks
                .iter()
                .filter(|(key, chunk)| {
                    (key.minimum.x() - eye.x as i32).abs() < MAX_DISTANCE_FROM_CAMERA
                        && (key.minimum.y() - eye.y as i32).abs() < MAX_DISTANCE_FROM_CAMERA
                        && chunk.render_task.is_none()
                        && chunk.rendered_version < chunk.source_version
                })
                .map(|(key, _chunk)| key.clone())
                .collect();
            changed_keys.sort_unstable_by_key(|key| {
                max(
                    (key.minimum.x() - eye.x as i32).abs(),
                    (key.minimum.y() - eye.y as i32).abs(),
                )
            });
            let to_render: Vec<(_, Array3x1<CubeVoxel>)> = changed_keys
                .iter()
                .take(if self.initialized {
                    min(
                        MAX_NEW_RENDER_CHUNK_JOBS_PER_FRAME,
                        MAX_RENDER_CHUNK_JOBS - current_jobs,
                    )
                } else {
                    MAX_RENDER_CHUNK_JOBS_INIT
                })
                .map(|key| {
                    let padded_chunk_extent = padded_greedy_quads_chunk_extent(
                        &self.voxels.indexer.extent_for_chunk_with_min(key.minimum),
                    );
                    let mut padded_chunk = Array3x1::fill(padded_chunk_extent, CubeVoxel(0));
                    copy_extent(
                        &padded_chunk_extent,
                        &self.voxels.lod_view(0),
                        &mut padded_chunk,
                    );
                    (key.clone(), padded_chunk)
                })
                .collect();
            if to_render.len() > 0 {
                let extract_time = (Instant::now() - extract_start).as_micros() as u32;
                log::debug!(
                    "Starting {} greedy mesh jobs (data extraction took {}µs)",
                    to_render.len(),
                    extract_time
                );
                self.metrics.extract.push(RenderChunkExtractMetrics {
                    tasks: to_render.len() as u32,
                    extract_time,
                });
                self.initialized = true;
            }

            for (key, padded_chunk) in to_render {
                let render_tx = self.render_tx.clone();
                let config = self.materials.clone();
                let padded_extent = padded_chunk.extent().clone();
                let task = self.task_pool.spawn(async move {
                    let quads_start = Instant::now();
                    let mut buffer = GreedyQuadsBuffer::new(
                        padded_extent,
                        RIGHT_HANDED_Y_UP_CONFIG.quad_groups(),
                    );
                    greedy_quads(&padded_chunk, &padded_extent, &mut buffer);
                    let quads_duration = Instant::now() - quads_start;
                    let mesh_start = Instant::now();
                    let (mesh, failed) = if buffer.num_quads() == 0 {
                        (None, false)
                    } else {
                        let mesh = Self::make_dyn_mesh_data(&padded_chunk, &buffer, &config);
                        let failed = mesh.is_none();
                        (mesh, failed)
                    };
                    let mesh_duration = Instant::now() - mesh_start;
                    let results = RenderChunkTaskResults {
                        key: key.clone(),
                        mesh,
                        metrics: RenderChunkTaskMetrics {
                            quads_time: quads_duration.as_micros() as u32,
                            mesh_time: mesh_duration.as_micros() as u32,
                            results_time: 0,
                            failed,
                        },
                    };
                    let _result = render_tx.send(results);
                });
                if let Some(chunk) = self.render_chunks.get_mut(&key) {
                    chunk.render_task = Some(task);
                }
            }
        }

        // process render job results
        for result in self.render_rx.try_iter() {
            let results_start = Instant::now();
            let mut metrics = result.metrics;

            if let Some(chunk) = self.render_chunks.get_mut(&result.key) {
                chunk.render_task = None;
                chunk.rendered_version += 1;

                // log::info!("Dyn mesh built. {}", result.mesh.clone());

                if let Some(mesh) = result.mesh {
                    let visible_bounds = mesh.visible_bounds.clone();
                    if let Some(handle) = &chunk.dyn_mesh_handle {
                        if let Err(error) = dyn_mesh_resource.update_dyn_mesh(&handle, mesh) {
                            log::error!("{}", error);
                        }
                    } else if let Ok(handle) = dyn_mesh_resource.add_dyn_mesh(mesh) {
                        chunk.dyn_mesh_handle = Some(handle.clone());

                        let transform_component = TransformComponent {
                            translation: Vec3::ZERO,
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
                            let pos = result.key.minimum;
                            handle.set_transform(
                                Vec3::new(pos.x() as f32, pos.y() as f32, pos.z() as f32),
                                Quat::IDENTITY,
                                Vec3::ONE,
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
                } else {
                    chunk.clear(world);
                }
            } else {
                metrics.failed = true;
            };

            metrics.results_time = (Instant::now() - results_start).as_micros() as u32;
            self.metrics.tasks.push(metrics);
        }

        self.check_reset_metrics(5.0, true);
    }

    pub fn check_reset_metrics(
        &mut self,
        interval_secs: f64,
        info_log: bool,
    ) -> Option<RenderChunkDistributionMetrics> {
        if self.metrics.is_empty() {
            self.metrics.start = Instant::now();
            return None;
        }
        let duration = Instant::now() - self.metrics.start;
        if duration.as_secs_f64() >= interval_secs {
            let metrics = self.metrics.get_distribution_metrics();
            if info_log {
                metrics.info_log();
            }
            self.metrics = Default::default();
            Some(metrics)
        } else {
            None
        }
    }

    fn make_dyn_mesh_data(
        voxels: &Array3x1<CubeVoxel>,
        quads: &GreedyQuadsBuffer,
        materials: &Vec<PbrMaterialAsset>,
    ) -> Option<DynMeshData> {
        let mut quad_parts: FnvHashMap<_, _> = Default::default();
        for (idx, group) in quads.quad_groups.iter().enumerate() {
            for quad in group.quads.iter() {
                let mat = voxels.get(quad.minimum);
                assert_ne!(mat.0, 0);
                let entry = quad_parts
                    .entry(mat.0 - 1)
                    .or_insert(PerMaterialGreedyQuadsBuffer::new(mat));
                entry.quad_groups[idx].quads.push(quad.clone());
            }
        }

        let num_quads = quads.num_quads();
        let mut all_vertices = PushBuffer::new(num_quads * 4 * std::mem::size_of::<MeshVertex>());
        let mut all_indices = PushBuffer::new(
            num_quads
                * 6
                * if num_quads * 6 >= 0xFFFF {
                    std::mem::size_of::<u32>()
                } else {
                    std::mem::size_of::<u16>()
                },
        );

        let mut mesh_parts: Vec<DynMeshDataPart> = Vec::with_capacity(quad_parts.len());
        for (mat, quads) in quad_parts.iter() {
            let mesh_part = {
                let pbr_material = materials.get(*mat as usize);
                if let Some(pbr_material) = pbr_material {
                    let mut vertices_num = 0;
                    let index_type = if quads.num_quads() * 6 >= 0xFFFF {
                        RafxIndexType::Uint32
                    } else {
                        RafxIndexType::Uint16
                    };
                    let vertex_offset = all_vertices.len();
                    let indices_offset = all_indices.len();
                    for group in quads.quad_groups.iter() {
                        for quad in group.quads.iter() {
                            let face = &group.face;
                            let mut positions: Vec<[f32; 3]> = Vec::new();
                            positions.extend_from_slice(&face.quad_mesh_positions(quad, 1.0));
                            let normals = &face.quad_mesh_normals();
                            let mut uvs: Vec<[f32; 2]> = Vec::new();
                            uvs.extend_from_slice(&face.tex_coords(
                                RIGHT_HANDED_Y_UP_CONFIG.u_flip_face,
                                true,
                                quad,
                            ));
                            let indices_u32 = &face.quad_mesh_indices(vertices_num);
                            let tangent = {
                                let (ind0, ind1, ind2) = (
                                    (indices_u32[0] - vertices_num) as usize,
                                    (indices_u32[1] - vertices_num) as usize,
                                    (indices_u32[2] - vertices_num) as usize,
                                );
                                let (pos0, pos1, pos2) = (
                                    Vec3::from_slice_unaligned(&positions[ind0]),
                                    Vec3::from_slice_unaligned(&positions[ind1]),
                                    Vec3::from_slice_unaligned(&positions[ind2]),
                                );
                                let (uv0, uv1, uv2) = (
                                    Vec2::from_slice_unaligned(&uvs[ind0]),
                                    Vec2::from_slice_unaligned(&uvs[ind1]),
                                    Vec2::from_slice_unaligned(&uvs[ind2]),
                                );
                                let delta_pos1 = pos1 - pos0;
                                let delta_pos2 = pos2 - pos0;
                                let delta_uv1 = uv1 - uv0;
                                let delta_uv2 = uv2 - uv0;
                                let r =
                                    1.0 / (delta_uv1.x * delta_uv2.y - delta_uv1.y * delta_uv2.x);
                                let tangent =
                                    (delta_pos1 * delta_uv2.y - delta_pos2 * delta_uv1.y) * r;
                                tangent.normalize()
                            };

                            for i in 0..4 {
                                all_vertices.push(
                                    &[MeshVertex {
                                        position: positions[i],
                                        normal: normals[i],
                                        tangent: [tangent.x, tangent.y, tangent.z, -1.],
                                        tex_coord: uvs[i],
                                    }],
                                    4,
                                );
                            }
                            match index_type {
                                rafx::api::RafxIndexType::Uint16 => {
                                    let indices_u16: Vec<u16> = indices_u32
                                        .iter()
                                        .map(|&x| std::convert::TryInto::try_into(x).unwrap())
                                        .collect();
                                    all_indices.push(&indices_u16, std::mem::size_of::<u16>());
                                }
                                rafx::api::RafxIndexType::Uint32 => {
                                    all_indices.push(indices_u32, std::mem::size_of::<u32>());
                                }
                            }
                            vertices_num += 4;
                        }
                    }
                    let vertex_size = all_vertices.len() - vertex_offset;
                    let indices_size = all_indices.len() - indices_offset;

                    if vertex_size == 0 || indices_size == 0 {
                        None
                    } else {
                        Some(DynMeshDataPart {
                            material_instance: pbr_material.get_material_instance(),
                            vertex_buffer_offset_in_bytes: vertex_offset as u32,
                            vertex_buffer_size_in_bytes: vertex_size as u32,
                            index_buffer_offset_in_bytes: indices_offset as u32,
                            index_buffer_size_in_bytes: indices_size as u32,
                            index_type,
                        })
                    }
                } else {
                    log::error!(
                        "Invalid terrain material index {} (# of materials: {})",
                        mat,
                        materials.len()
                    );
                    None
                }
            };
            if let Some(mesh_part) = mesh_part {
                mesh_parts.push(mesh_part);
            } else {
                return None;
            }
        }

        if mesh_parts.len() == 0 {
            return None;
        }

        Some(DynMeshData {
            mesh_parts,
            vertex_buffer: Some(all_vertices.into_data()),
            index_buffer: Some(all_indices.into_data()),
            visible_bounds: Self::make_visible_bounds(&voxels.extent().padded(-1), 0),
        })
    }

    fn make_visible_bounds(extent: &Extent3i, hash: u64) -> VisibleBounds {
        let max = extent.shape;
        let max = Vec3::new(max.x() as f32, max.y() as f32, max.z() as f32) + Vec3::ONE;
        let sphere_center = max / 2.;
        let sphere_radius = sphere_center.distance(max);

        VisibleBounds {
            aabb: AxisAlignedBoundingBox {
                min: Vec3::ZERO,
                max,
            },
            obb: Default::default(),
            bounding_sphere: BoundingSphere::new(sphere_center, sphere_radius),
            hash,
        }
    }

    pub fn ray_cast(&self, start: Vec3, ray: Vec3) -> Option<RayCastResult> {
        let start = PointN([start.x, start.y, start.z]);
        let ray = PointN([ray.x, ray.y, ray.z]);
        let mut traversal = GridRayTraversal3::new(start, ray);
        let mut prev = PointN([start.x() as i32, start.y() as i32, start.z() as i32]);
        for _ in 0..256 {
            let current = traversal.current_voxel();
            let vox = self.voxels.get_point(0, current);
            if vox.0 != 0 {
                return Some(RayCastResult {
                    hit: current,
                    before_hit: prev,
                });
            }
            prev = current;
            traversal.step();
        }
        return None;
    }
}

pub struct RayCastResult {
    pub hit: Point3i,
    pub before_hit: Point3i,
}

pub struct PerMaterialGreedyQuadsBuffer {
    pub quad_groups: [QuadGroup; 6],
    pub material: CubeVoxel,
}

impl PerMaterialGreedyQuadsBuffer {
    pub fn num_quads(&self) -> usize {
        let mut sum = 0;
        for group in self.quad_groups.iter() {
            sum += group.quads.len();
        }

        sum
    }
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
pub enum TerrainFillStyle {
    FlatBoard {
        material: &'static str,
    },
    CheckersBoard {
        zero: &'static str,
        one: &'static str,
    },
    PerlinNoise {
        params: PerlinNoise2D,
        material: &'static str,
    },
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
        world: &mut World,
        materials: Vec<(&'static str, PbrMaterialAsset)>,
        origin: Point3i,
        size: u32,
        style: TerrainFillStyle,
    ) -> TerrainHandle {
        log::info!("Creating terrain...");

        let mut terrain = {
            let material_names = materials
                .iter()
                .enumerate()
                .map(|(idx, v)| (v.0.to_string(), idx as u16))
                .collect();
            let materials = materials.iter().map(|v| v.1.clone()).collect();
            let voxels = Terrain::generate_voxels(&material_names, origin, size, style);
            let (render_tx, render_rx) = unbounded();
            Terrain {
                materials,
                material_names,
                voxels,
                task_pool: TaskPoolBuilder::new().build(),
                render_chunks: HashMap::new(),
                render_tx,
                render_rx,
                metrics: Default::default(),
                initialized: false,
            }
        };

        terrain.reset_chunks(world);

        let terrain_handle = {
            let mut storage = self.write();
            storage.register_terrain(terrain)
        };

        log::info!("Terrain created");

        terrain_handle
    }
}

#[derive(Clone)]
pub struct TerrainComponent {
    pub handle: TerrainHandle,
}
