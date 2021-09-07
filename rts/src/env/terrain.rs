use std::{
    cmp::{max, min},
    collections::{HashMap, HashSet},
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
use distill::loader::handle::Handle;
use fnv::FnvHashMap;
use glam::{Quat, Vec3};
use legion::{Entity, Resources, World};
use rafx::{
    api::RafxIndexType,
    assets::{push_buffer::PushBuffer, AssetManager},
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
    assets::{
        pbr_material::PbrMaterialAsset,
        tile::{TileAsset, TileExporter},
    },
    env::perlin::PerlinNoise2D,
    features::dyn_mesh::{
        DynMeshCommand, DynMeshCommandResults, DynMeshData, DynMeshDataPart, DynMeshHandle,
        DynMeshManager, DynMeshRenderObject, DynMeshRenderObjectSet,
    },
};

#[derive(Clone, Copy, Default)]
pub struct TerrainVoxel(u16);

impl TerrainVoxel {
    pub fn empty() -> Self {
        Self(0)
    }

    pub fn from_material_index(material: u16) -> Self {
        Self(material)
    }
}

impl MergeVoxel for TerrainVoxel {
    type VoxelValue = u16;

    fn voxel_merge_value(&self) -> Self::VoxelValue {
        self.0
    }
}

impl IsOpaque for TerrainVoxel {
    fn is_opaque(&self) -> bool {
        true
    }
}

impl IsEmpty for TerrainVoxel {
    fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

struct TerrainChunkTaskMetrics {
    pub quads_time: u32, // µs
    pub mesh_time: u32,  // µs
    pub failed: bool,
}

struct TerrainChunkExtractMetrics {
    pub tasks: u32,
    pub extract_time: u32, // µs
}

struct SingleDistributionMetrics {
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

struct TerrainChunkDistributionMetrics {
    pub extract_time: SingleDistributionMetrics,
    pub quads_time: SingleDistributionMetrics,
    pub mesh_time: SingleDistributionMetrics,
}

impl TerrainChunkDistributionMetrics {
    pub fn info_log(&self) {
        self.extract_time.info_log("extract");
        self.quads_time.info_log("quads");
        self.mesh_time.info_log("mesh");
    }
}

struct TerrainChunkMetrics {
    pub start: Instant,
    pub tasks: Vec<TerrainChunkTaskMetrics>,
    pub extract: Vec<TerrainChunkExtractMetrics>,
}

impl Default for TerrainChunkMetrics {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            tasks: Default::default(),
            extract: Default::default(),
        }
    }
}

impl TerrainChunkMetrics {
    pub fn is_empty(&self) -> bool {
        self.extract.is_empty() && self.tasks.is_empty()
    }

    pub fn get_distribution_metrics(&self) -> TerrainChunkDistributionMetrics {
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

        TerrainChunkDistributionMetrics {
            extract_time,
            quads_time,
            mesh_time,
        }
    }
}

struct TerrainChunkTaskResults {
    pub key: ChunkKey3,
    pub mesh: Option<DynMeshData>,
    pub metrics: TerrainChunkTaskMetrics,
}

struct TerrainChunk {
    pub entity: Option<Entity>,
    pub dyn_mesh_handle: Option<DynMeshHandle>,
    pub render_object_handle: Option<RenderObjectHandle>,
    pub visibility_object_handle: Option<VisibilityObjectArc>,
    pub dirty: bool,
    pub builder: Option<Task<()>>,
}

impl TerrainChunk {
    pub fn new() -> Self {
        TerrainChunk {
            entity: None,
            dyn_mesh_handle: None,
            render_object_handle: None,
            visibility_object_handle: None,
            dirty: false,
            builder: None,
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

pub type TerrainVoxels = ChunkHashMap3<TerrainVoxel, ChunkMapBuilder3x1<TerrainVoxel>>;

pub struct Terrain {
    initialized: bool,
    materials: Vec<Handle<PbrMaterialAsset>>,
    material_names: Vec<String>,
    materials_map: HashMap<String, u16>,
    voxels: TerrainVoxels,
    task_pool: TaskPool,
    active_builders: usize,
    chunks: HashMap<ChunkKey3, TerrainChunk>,
    super_chunks: HashMap<Point3i, HashSet<ChunkKey3>>,
    builder_tx: Sender<TerrainChunkTaskResults>,
    builder_rx: Receiver<TerrainChunkTaskResults>,
    metrics: TerrainChunkMetrics,
    dyn_mesh_cmd_tx: Sender<DynMeshCommand>,
    dyn_mesh_cmd_rx: Receiver<DynMeshCommandResults>,
    dyn_mesh_add_requests: HashMap<usize, (ChunkKey3, VisibleBounds)>,
    current_dyn_mesh_add_request: usize,
}

const MAX_RENDER_CHUNK_JOBS: usize = 16;
const MAX_NEW_RENDER_CHUNK_JOBS_PER_FRAME: usize = 4;
const MAX_RENDER_CHUNK_JOBS_INIT: usize = 65536;
const MAX_DISTANCE_FROM_CAMERA: i32 = 256;
const SUPER_CHUNK_SIZE: i32 = 256;
const TILE_EDIT_PLATFORM_SIZE: i32 = 32;

impl Terrain {
    pub fn get_default_material_names() -> Vec<&'static str> {
        vec![
            "flat_red",
            "flat_green",
            "flat_blue",
            "blue_metal",
            "old_bronze",
            "basic_tile",
            "round_tile",
            "diamond_inlay_tile",
            "black_plastic",
            "curly_tile",
        ]
    }

    pub fn get_pallete_voxel_string(
        &self,
        voxel: &TerrainVoxel,
        pallete: &mut Vec<String>,
        pallete_builder: &mut HashMap<String, u8>,
    ) -> String {
        if voxel.is_empty() {
            "00".to_string()
        } else {
            let mat = self.material_name_by_voxel(voxel);
            let entry = pallete_builder.entry(mat.clone()).or_insert_with(|| {
                pallete.push(mat);
                pallete.len() as u8
            });
            format!("{:02X}", entry)
        }
    }

    pub fn material_name_by_voxel(&self, voxel: &TerrainVoxel) -> String {
        if voxel.is_empty() {
            "".to_string()
        } else {
            self.material_names[voxel.0 as usize - 1].clone()
        }
    }

    pub fn voxel_by_material(&self, material_name: &str) -> Option<TerrainVoxel> {
        self.materials_map
            .get(material_name)
            .and_then(|idx| Some(TerrainVoxel(*idx + 1)))
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

    pub fn update_voxel(&mut self, point: Point3i, voxel: TerrainVoxel) {
        let vox_ref: &mut TerrainVoxel = self.voxels.get_mut_point(0, point);
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
        self.update_voxel(point, TerrainVoxel::empty());
    }

    pub fn instance_tile(&mut self, tile: &TileAsset, position: Point3i) {
        let pallete: Vec<_> = tile
            .inner
            .palette
            .iter()
            .map(|mat_name| self.voxel_by_material(mat_name).unwrap())
            .collect();

        let mut voxels = tile.inner.voxels.clone();
        let mut center = voxels.extent().shape / 2;
        *center.z_mut() = 0;
        voxels.set_minimum(position - center);
        let extent = voxels.extent().clone();
        voxels.for_each_mut(&extent, |_p: Point3i, vox: &mut TerrainVoxel| {
            if !vox.is_empty() {
                *vox = pallete[vox.0 as usize - 1];
            }
        });
        copy_extent(&extent, &voxels, &mut self.voxels.lod_view_mut(0));

        let mut chunks = vec![];
        self.voxels
            .visit_occupied_chunks(0, &voxels.extent().padded(1), |chunk| {
                chunks.push(ChunkKey3::new(0, chunk.extent().minimum));
            });
        for chunk_key in chunks {
            self.set_chunk_dirty(chunk_key);
        }
    }

    pub fn save_edited_tile(&self, tile: &str) -> Option<()> {
        let full_extent = Extent3i::from_min_and_shape(
            PointN([
                -TILE_EDIT_PLATFORM_SIZE / 2,
                -TILE_EDIT_PLATFORM_SIZE / 2,
                0,
            ]),
            Point3i::fill(TILE_EDIT_PLATFORM_SIZE),
        );

        let mut min = PointN([TILE_EDIT_PLATFORM_SIZE, TILE_EDIT_PLATFORM_SIZE, 0]);
        let mut max = Point3i::fill(-TILE_EDIT_PLATFORM_SIZE);
        for p in full_extent.iter_points() {
            let v = self.voxels.get_point(0, p);
            if !v.is_empty() {
                if p.x() < min.x() {
                    *min.x_mut() = p.x();
                }
                if p.y() < min.y() {
                    *min.y_mut() = p.y();
                }
                if p.x() > max.x() {
                    *max.x_mut() = p.x();
                }
                if p.y() > max.y() {
                    *max.y_mut() = p.y();
                }
                if p.z() > max.z() {
                    *max.z_mut() = p.z();
                }
            }
        }
        let extent = Extent3i::from_min_and_max(min, max);

        let mut export_voxels = Array3x1::<TerrainVoxel>::fill(extent, TerrainVoxel::empty());
        copy_extent(&extent, &self.voxels.lod_view(0), &mut export_voxels);

        TileExporter::export(tile.to_string(), export_voxels, self)
    }

    pub fn reset(
        &mut self,
        world: &mut World,
        origin: Point3i,
        size: u32,
        style: TerrainFillStyle,
    ) {
        log::info!("Resetting terrain...");

        self.voxels = Self::generate_voxels(&self.materials_map, origin, size, style);
        self.reset_chunks(world);

        log::info!("Terrain reset");
    }

    fn reset_chunks(&mut self, world: &mut World) {
        self.active_builders = 0;
        self.super_chunks.clear();
        for chunk in self.chunks.values_mut() {
            chunk.clear(world);
        }
        self.chunks.clear();
        let full_extent = self.voxels.bounding_extent(0);
        let mut occupied = vec![];
        self.voxels.visit_occupied_chunks(0, &full_extent, |chunk| {
            occupied.push(chunk.extent().minimum);
        });
        for chunk_min in occupied {
            self.set_chunk_dirty(ChunkKey3::new(0, chunk_min));
        }
    }

    fn generate_voxels(
        materials: &HashMap<String, u16>,
        origin: Point3i,
        size: u32,
        style: TerrainFillStyle,
    ) -> TerrainVoxels {
        let chunk_shape = Point3i::fill(16);
        let ambient_value = TerrainVoxel::default();
        let builder = ChunkMapBuilder3x1::new(chunk_shape, ambient_value);
        let mut voxels = builder.build_with_hash_map_storage();
        let mut lod0 = voxels.lod_view_mut(0);
        let size = size as i32;
        let base_min = PointN([origin.x() - size / 2, origin.y() - size / 2, origin.z() - 1]);
        let base_extent = Extent3i::from_min_and_shape(base_min, PointN([size, size, 1]));
        match style {
            TerrainFillStyle::FlatBoard { material } => {
                let voxel = TerrainVoxel(materials[material] + 1);
                lod0.fill_extent(&base_extent, voxel);
            }
            TerrainFillStyle::CheckersBoard { zero, one } => {
                let zero_voxel = TerrainVoxel(materials[zero] + 1);
                let one_voxel = TerrainVoxel(materials[one] + 1);
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
                let voxel = TerrainVoxel(materials[material] + 1);
                for p in base_extent.iter_points() {
                    let noise = params.get_noise(p.x() as f64, p.y() as f64) as i32;
                    let top = PointN([p.x(), p.y(), noise - 8]);
                    lod0.fill_extent(&Extent3i::from_min_and_shape(top, PointN([1, 1, 8])), voxel);
                }
            }
        };
        voxels
    }

    fn get_super_chunk_key(chunk: &ChunkKey3) -> Point3i {
        let c = chunk.minimum;
        let p = c / SUPER_CHUNK_SIZE;
        SUPER_CHUNK_SIZE
            * PointN([
                if c.x() < 0 { p.x() - 1 } else { p.x() },
                if c.y() < 0 { p.y() - 1 } else { p.y() },
                if c.z() < 0 { p.z() - 1 } else { p.z() },
            ])
    }

    fn set_chunk_dirty(&mut self, key: ChunkKey3) {
        self.super_chunks
            .entry(Self::get_super_chunk_key(&key))
            .or_insert(HashSet::new())
            .insert(key);
        let chunk = self.chunks.entry(key).or_insert(TerrainChunk::new());
        chunk.dirty = true;
    }

    #[profiling::function]
    pub fn update_chunks(&mut self, world: &mut World, resources: &Resources) {
        self.start_mesh_jobs(resources);
        self.process_job_results(world, resources);
        self.check_reset_metrics(5.0, true);
    }

    #[profiling::function]
    fn extract_mesh_voxels(
        &mut self,
        resources: &Resources,
    ) -> Vec<(ChunkKey<[i32; 3]>, Array3x1<TerrainVoxel>)> {
        let viewports_resource = resources.get::<ViewportsResource>().unwrap();
        let eye = viewports_resource
            .main_view_meta
            .as_ref()
            .and_then(|view| Some(view.eye_position))
            .unwrap_or_default();
        let eye = PointN([eye.x as i32, eye.y as i32, eye.z as i32]);

        let mut changed_keys = vec![];
        let super_center = Point3i::fill(SUPER_CHUNK_SIZE / 2);
        for (key, chunk_set) in self.super_chunks.iter() {
            let center = *key + super_center;
            if (center.x() - eye.x()).abs() <= MAX_DISTANCE_FROM_CAMERA + SUPER_CHUNK_SIZE
                && (center.y() - eye.y()).abs() <= MAX_DISTANCE_FROM_CAMERA + SUPER_CHUNK_SIZE
            {
                for chunk_key in chunk_set {
                    if (chunk_key.minimum.x() - eye.x()).abs() <= MAX_DISTANCE_FROM_CAMERA
                        && (chunk_key.minimum.y() - eye.y()).abs() <= MAX_DISTANCE_FROM_CAMERA
                    {
                        let chunk = self.chunks.get(chunk_key).unwrap();
                        if chunk.builder.is_none() && chunk.dirty {
                            changed_keys.push(chunk_key.clone());
                        }
                    }
                }
            }
        }
        changed_keys.sort_unstable_by_key(|key| {
            max(
                (key.minimum.x() - eye.x()).abs(),
                (key.minimum.y() - eye.y()).abs(),
            )
        });

        changed_keys
            .iter()
            .take(if self.initialized {
                min(
                    MAX_NEW_RENDER_CHUNK_JOBS_PER_FRAME,
                    MAX_RENDER_CHUNK_JOBS - self.active_builders,
                )
            } else {
                MAX_RENDER_CHUNK_JOBS_INIT
            })
            .map(|key| {
                let padded_chunk_extent = padded_greedy_quads_chunk_extent(
                    &self.voxels.indexer.extent_for_chunk_with_min(key.minimum),
                );
                let mut padded_chunk = Array3x1::fill(padded_chunk_extent, TerrainVoxel::empty());
                copy_extent(
                    &padded_chunk_extent,
                    &self.voxels.lod_view(0),
                    &mut padded_chunk,
                );
                (key.clone(), padded_chunk)
            })
            .collect()
    }

    #[profiling::function]
    fn start_mesh_jobs(&mut self, resources: &Resources) {
        if !self.initialized || self.active_builders < MAX_RENDER_CHUNK_JOBS {
            let extract_start = Instant::now();
            let to_render = self.extract_mesh_voxels(resources);

            if to_render.len() > 0 {
                let extract_time = (Instant::now() - extract_start).as_micros() as u32;
                log::debug!(
                    "Starting {} greedy mesh jobs (data extraction took {}µs)",
                    to_render.len(),
                    extract_time
                );
                self.metrics.extract.push(TerrainChunkExtractMetrics {
                    tasks: to_render.len() as u32,
                    extract_time,
                });
                self.initialized = true;

                let asset_manager = resources.get::<AssetManager>().unwrap();
                let materials: Vec<_> = self
                    .materials
                    .iter()
                    .map(|h| {
                        asset_manager
                            .committed_asset(h)
                            .and_then(|m| Some(m.clone()))
                    })
                    .collect();

                for (key, padded_chunk) in to_render {
                    let builder_tx = self.builder_tx.clone();
                    let materials = materials.clone();
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
                            let mesh = Self::make_dyn_mesh_data(&padded_chunk, &buffer, &materials);
                            let failed = mesh.is_none();
                            (mesh, failed)
                        };
                        let mesh_duration = Instant::now() - mesh_start;
                        let results = TerrainChunkTaskResults {
                            key: key.clone(),
                            mesh,
                            metrics: TerrainChunkTaskMetrics {
                                quads_time: quads_duration.as_micros() as u32,
                                mesh_time: mesh_duration.as_micros() as u32,
                                failed,
                            },
                        };
                        let _result = builder_tx.send(results);
                    });
                    if let Some(chunk) = self.chunks.get_mut(&key) {
                        chunk.builder = Some(task);
                        chunk.dirty = false;
                        self.active_builders += 1;
                    }
                }
            }
        }
    }

    #[profiling::function]
    fn process_job_results(&mut self, world: &mut World, resources: &Resources) {
        let mut dyn_mesh_render_objects = resources.get_mut::<DynMeshRenderObjectSet>().unwrap();
        let visibility_region = resources.get::<VisibilityRegion>().unwrap();

        let mut cleared_chunks = vec![];
        for result in self.builder_rx.try_iter() {
            let mut metrics = result.metrics;

            if let Some(chunk) = self.chunks.get_mut(&result.key) {
                chunk.builder = None;
                self.active_builders -= 1;
                if let Some(mesh) = result.mesh {
                    if let Some(handle) = &chunk.dyn_mesh_handle {
                        let _res = self.dyn_mesh_cmd_tx.send(DynMeshCommand::Update {
                            request_handle: 0,
                            handle: handle.clone(),
                            data: mesh,
                        });
                    } else {
                        self.current_dyn_mesh_add_request += 1;
                        let request_handle = self.current_dyn_mesh_add_request;
                        self.dyn_mesh_add_requests
                            .insert(request_handle, (result.key, mesh.visible_bounds.clone()));
                        let _res = self.dyn_mesh_cmd_tx.send(DynMeshCommand::Add {
                            request_handle,
                            data: mesh,
                        });
                    }
                } else {
                    chunk.clear(world);
                    cleared_chunks.push(result.key.clone());
                }
            } else {
                metrics.failed = true;
            };
            self.metrics.tasks.push(metrics);
        }

        for result in self.dyn_mesh_cmd_rx.try_iter() {
            match result {
                DynMeshCommandResults::Add {
                    request_handle,
                    result,
                } => {
                    if let Some((key, visible_bounds)) =
                        self.dyn_mesh_add_requests.remove(&request_handle)
                    {
                        if let Some(chunk) = self.chunks.get_mut(&key) {
                            match result {
                                Ok(handle) => {
                                    chunk.dyn_mesh_handle = Some(handle.clone());

                                    let transform_component = TransformComponent {
                                        translation: Vec3::ZERO,
                                        scale: Vec3::ONE,
                                        rotation: Quat::IDENTITY,
                                    };

                                    let render_object_handle = dyn_mesh_render_objects
                                        .register_render_object(DynMeshRenderObject {
                                            mesh: handle,
                                        });
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
                                        let pos = key.minimum;
                                        handle.set_transform(
                                            Vec3::new(
                                                pos.x() as f32,
                                                pos.y() as f32,
                                                pos.z() as f32,
                                            ),
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
                                Err(err) => log::error!("{}", err),
                            }
                        }
                    };
                }
                DynMeshCommandResults::Update {
                    request_handle: _,
                    result,
                } => {
                    if let Err(error) = result {
                        log::error!("{}", error);
                    }
                }
            }
        }

        for chunk in cleared_chunks {
            let super_key = Self::get_super_chunk_key(&chunk);
            if let Some(super_chunk) = self.super_chunks.get_mut(&super_key) {
                super_chunk.remove(&chunk);
                if super_chunk.is_empty() {
                    self.super_chunks.remove(&super_key);
                }
            }
        }
    }

    fn check_reset_metrics(
        &mut self,
        interval_secs: f64,
        info_log: bool,
    ) -> Option<TerrainChunkDistributionMetrics> {
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

    #[profiling::function]
    fn make_dyn_mesh_data(
        voxels: &Array3x1<TerrainVoxel>,
        quads: &GreedyQuadsBuffer,
        materials: &Vec<Option<PbrMaterialAsset>>,
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
                if let Some(Some(pbr_material)) = pbr_material {
                    let mut vertices_num = 0;
                    let index_type = if quads.num_quads() * 6 >= 0xFFFF {
                        RafxIndexType::Uint32
                    } else {
                        RafxIndexType::Uint16
                    };
                    let vertex_offset = all_vertices.len();
                    let indices_offset = all_indices.len();
                    for group in quads.quad_groups.iter() {
                        let face = &group.face;
                        let normal = face.mesh_normal().0;
                        let tangent = {
                            let face_normal_axis = face.permutation.axes()[0];
                            let flip_u = if face.n_sign < 0 {
                                RIGHT_HANDED_Y_UP_CONFIG.u_flip_face != face_normal_axis
                            } else {
                                RIGHT_HANDED_Y_UP_CONFIG.u_flip_face == face_normal_axis
                            };
                            let flipped_u = if flip_u { -face.u } else { face.u };
                            [
                                flipped_u.x() as f32,
                                flipped_u.y() as f32,
                                flipped_u.z() as f32,
                                1., // right handed
                            ]
                        };
                        for quad in group.quads.iter() {
                            let mut positions: Vec<[f32; 3]> = Vec::new();
                            positions.extend_from_slice(&face.quad_mesh_positions(quad, 1.0));
                            let mut uvs: Vec<[f32; 2]> = Vec::new();
                            uvs.extend_from_slice(&face.tex_coords(
                                RIGHT_HANDED_Y_UP_CONFIG.u_flip_face,
                                false,
                                quad,
                            ));
                            let indices_u32 = &face.quad_mesh_indices(vertices_num);
                            for i in 0..4 {
                                all_vertices.push(
                                    &[MeshVertex {
                                        position: positions[i],
                                        normal,
                                        tangent,
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
}

pub struct RayCastResult {
    pub hit: Point3i,
    pub before_hit: Point3i,
}

struct PerMaterialGreedyQuadsBuffer {
    pub quad_groups: [QuadGroup; 6],
    pub material: TerrainVoxel,
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
    pub fn new(material: TerrainVoxel) -> Self {
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

    fn register_terrain(&mut self, terrain: Terrain) -> TerrainHandle {
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
        dyn_mesh_manager: &DynMeshManager,
        materials: Vec<(&'static str, Handle<PbrMaterialAsset>)>,
        origin: Point3i,
        size: u32,
        style: TerrainFillStyle,
    ) -> TerrainHandle {
        log::info!("Creating terrain...");

        let mut terrain = {
            let material_names = materials
                .iter()
                .map(|(name, _h)| name.to_string())
                .collect();
            let materials_map = materials
                .iter()
                .enumerate()
                .map(|(idx, v)| (v.0.to_string(), idx as u16))
                .collect();
            let materials = materials.iter().map(|v| v.1.clone()).collect();
            let voxels = Terrain::generate_voxels(&materials_map, origin, size, style);
            let (render_tx, render_rx) = unbounded();
            let (dyn_mesh_cmd_tx, dyn_mesh_cmd_rx) = dyn_mesh_manager.get_command_channels();
            Terrain {
                initialized: false,
                materials,
                material_names,
                materials_map,
                voxels,
                task_pool: TaskPoolBuilder::new().build(),
                active_builders: 0,
                chunks: HashMap::new(),
                super_chunks: HashMap::new(),
                builder_tx: render_tx,
                builder_rx: render_rx,
                metrics: Default::default(),
                dyn_mesh_cmd_tx,
                dyn_mesh_cmd_rx,
                dyn_mesh_add_requests: HashMap::new(),
                current_dyn_mesh_add_request: 0,
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
