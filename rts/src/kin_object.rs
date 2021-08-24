use crate::{
    assets::pbr_material::PbrMaterialAsset,
    camera::RTSCamera,
    input::{InputResource, KeyboardKey, MouseButton},
    terrain::{CubeVoxel, Terrain, TerrainFillStyle, TerrainHandle, TerrainResource},
};
use building_blocks::{core::prelude::*, storage::prelude::*};
use egui::{Button, Checkbox};
use glam::{Quat, Vec3};
use legion::{Resources, World};
use rafx::assets::{distill_impl::AssetResource, AssetManager};
use rafx_plugins::{components::TransformComponent, features::egui::EguiContextResource};
use std::collections::HashMap;

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum KinObjectType {
    Building,
    Tree,
}

#[derive(Clone)]
pub struct KinObjectComponent {
    pub object_type: KinObjectType,
    pub health: f32,
    pub selected: bool,
}

pub struct KinObjectsState {
    pub terrain: TerrainHandle,
    objects: HashMap<KinObjectType, Array3x1<CubeVoxel>>,
    ui_spawning: bool,
    ui_object_type: KinObjectType,
    ui_edit_mode: bool,
    ui_edit_material: &'static str,
}

impl KinObjectsState {
    pub fn new(resources: &Resources) -> Self {
        let mut asset_manager = resources.get_mut::<AssetManager>().unwrap();
        let mut asset_resource = resources.get_mut::<AssetResource>().unwrap();

        log::info!("Loading terrain materials...");

        let material_names = Terrain::get_default_material_names();
        let terrain_materials: Vec<_> = material_names
            .iter()
            .map(|name| {
                let path = format!("materials/terrain/{}.pbrmaterial", *name);
                let material_handle = asset_resource.load_asset_path::<PbrMaterialAsset, _>(path);
                asset_manager
                    .wait_for_asset_to_load(&material_handle, &mut asset_resource, "")
                    .unwrap();
                (
                    *name,
                    asset_manager
                        .committed_asset(&material_handle)
                        .unwrap()
                        .clone(),
                )
            })
            .collect();

        log::info!("Terrain materials loaded");

        let terrain_handle = {
            let mut terrain_resource = resources.get_mut::<TerrainResource>().unwrap();
            let w = 4096;
            terrain_resource.new_terrain(
                terrain_materials,
                Extent3i::from_min_and_shape(PointN([-w / 2, -w / 2, -1]), PointN([w, w, 1])),
                TerrainFillStyle::Checkers {
                    zero: "simple_tile",
                    one: "black_plastic",
                },
            )
        };
        let terrain_resource = resources.get::<TerrainResource>().unwrap();
        let storage = terrain_resource.read();
        let terrain = storage.get(&terrain_handle);

        let empty = 0.into();
        let dimond_tile = terrain.voxel_by_material("diamond-inlay-tile").unwrap();
        let round_tile = terrain
            .voxel_by_material("round-pattern-wallpaper")
            .unwrap();
        let curly_tile = terrain.voxel_by_material("curly_tile").unwrap();
        let black_plastic = terrain.voxel_by_material("black_plastic").unwrap();

        let mut objects = HashMap::new();

        let mut building = Array3x1::<CubeVoxel>::fill(
            Extent3i::from_min_and_shape(Point3i::ZERO, PointN([8, 8, 8])),
            empty,
        );
        building.fill_extent(
            &Extent3i::from_min_and_shape(Point3i::ZERO, PointN([8, 8, 4])),
            dimond_tile,
        );
        building.fill_extent(
            &Extent3i::from_min_and_shape(PointN([1, 1, 4]), PointN([6, 6, 3])),
            dimond_tile,
        );
        building.fill_extent(
            &Extent3i::from_min_and_shape(PointN([1, 1, 7]), PointN([6, 6, 1])),
            round_tile,
        );
        objects.insert(KinObjectType::Building, building);

        let mut tree = Array3x1::<CubeVoxel>::fill(
            Extent3i::from_min_and_shape(PointN([-2, -2, 0]), PointN([5, 5, 9])),
            empty,
        );
        tree.fill_extent(
            &Extent3i::from_min_and_shape(Point3i::ZERO, PointN([1, 1, 4])),
            black_plastic,
        );
        tree.fill_extent(
            &Extent3i::from_min_and_shape(PointN([-2, -2, 4]), PointN([5, 5, 3])),
            curly_tile,
        );
        tree.fill_extent(
            &Extent3i::from_min_and_shape(PointN([-1, -1, 7]), PointN([3, 3, 1])),
            curly_tile,
        );
        tree.fill_extent(
            &Extent3i::from_min_and_shape(PointN([0, 0, 8]), PointN([1, 1, 1])),
            curly_tile,
        );
        objects.insert(KinObjectType::Tree, tree);

        KinObjectsState {
            terrain: terrain_handle,
            objects,
            ui_spawning: false,
            ui_object_type: KinObjectType::Building,
            ui_edit_mode: false,
            ui_edit_material: "simple_tile",
        }
    }

    pub fn update(&mut self, world: &mut World, resources: &mut Resources) {
        let input = resources.get::<InputResource>().unwrap();
        let camera = resources.get::<RTSCamera>().unwrap();
        let context = resources.get::<EguiContextResource>().unwrap().context();

        profiling::scope!("egui");
        egui::Window::new("Kinematics")
            .default_pos([200., 40.])
            .default_width(100.)
            .resizable(false)
            .show(&context, |ui| {
                if self.ui_spawning {
                    ui.label("Click a location on the map to spawn kinematic object");
                } else {
                    ui.radio_value(
                        &mut self.ui_object_type,
                        KinObjectType::Building,
                        "Building",
                    );
                    ui.radio_value(&mut self.ui_object_type, KinObjectType::Tree, "Tree");
                    ui.add_space(10.);
                    if ui.add_sized([100., 30.], Button::new("Spawn")).clicked() {
                        self.ui_spawning = true;
                    }
                    ui.add_space(10.);
                    ui.separator();
                    let ck = Checkbox::new(&mut self.ui_edit_mode, "Edit mode");
                    ui.add(ck);
                    if self.ui_edit_mode {
                        for material_name in Terrain::get_default_material_names() {
                            ui.radio_value(
                                &mut self.ui_edit_material,
                                material_name,
                                material_name,
                            );
                        }
                    }
                }
            });

        if self.ui_spawning || self.ui_edit_mode {
            if input.is_mouse_button_just_clicked(MouseButton::LEFT) {
                let cursor_pos = input.mouse_position();
                let (cast_result, default_material) = {
                    let terrain_resource = resources.get::<TerrainResource>().unwrap();
                    let storage = terrain_resource.read();
                    let terrain = storage.get(&self.terrain);
                    let cast_result =
                        camera.ray_cast_terrain(cursor_pos.x as u32, cursor_pos.y as u32, terrain);
                    let default_material =
                        terrain.voxel_by_material(self.ui_edit_material).unwrap();
                    (cast_result, default_material)
                };
                if let Some(result) = cast_result {
                    if self.ui_spawning {
                        self.spawn(
                            self.ui_object_type,
                            PointN([result.hit.x(), result.hit.y(), result.hit.z() + 1]),
                            resources,
                            world,
                        );
                    } else if self.ui_edit_mode {
                        let mut terrain_resource = resources.get_mut::<TerrainResource>().unwrap();
                        let mut storage = terrain_resource.write();
                        let terrain = storage.get_mut(&self.terrain);
                        if input.is_key_down(KeyboardKey::LControl) {
                            terrain.clear_voxel(result.hit);
                        } else {
                            terrain.update_voxel(result.before_hit, default_material);
                        }
                    }
                }
                self.ui_spawning = false;
            }
        }

        {
            profiling::scope!("update render chunks");
            let mut terrain_resource = resources.get_mut::<TerrainResource>().unwrap();
            let mut storage = terrain_resource.write();
            let terrain = storage.get_mut(&self.terrain);
            terrain.update_render_chunks(world, resources);
        }
    }

    pub fn spawn(
        &self,
        object_type: KinObjectType,
        position: Point3i,
        resources: &Resources,
        world: &mut World,
    ) {
        // transform component
        let translation = Vec3::new(
            position.x() as f32,
            position.y() as f32,
            position.z() as f32,
        );
        let transform_component = TransformComponent {
            translation,
            scale: Vec3::ONE,
            rotation: Quat::IDENTITY,
        };

        // kin object component
        let kin_object_component = KinObjectComponent {
            object_type,
            health: 1.,
            selected: false,
        };

        // entity
        log::info!("Spawn entity {:?} at: {}", object_type, translation);
        let _entity = world.push((transform_component, kin_object_component));

        // update voxels
        let mut terrain_resource = resources.get_mut::<TerrainResource>().unwrap();
        let mut storage = terrain_resource.write();
        let terrain = storage.get_mut(&self.terrain);

        let mut object = self.objects.get(&object_type).unwrap().clone();
        let mut half_size = object.extent().shape / 2;
        *half_size.z_mut() = 0;
        object.set_minimum(position - half_size);
        copy_extent(
            &object.extent(),
            &object,
            &mut terrain.voxels.lod_view_mut(0),
        );

        // set chunks dirty
        let mut chunks = vec![];
        terrain
            .voxels
            .visit_occupied_chunks(0, &object.extent().padded(1), |chunk| {
                chunks.push(ChunkKey3::new(0, chunk.extent().minimum));
            });
        for chunk_key in chunks {
            terrain.set_chunk_dirty(chunk_key);
        }
    }
}
