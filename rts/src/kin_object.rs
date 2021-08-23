use crate::{
    assets::pbr_material::PbrMaterialAsset,
    camera::RTSCamera,
    input::{InputResource, MouseButton},
    terrain::{CubeVoxel, TerrainHandle, TerrainResource},
};
use building_blocks::{core::prelude::*, storage::prelude::*};
use egui::Button;
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
}

impl KinObjectsState {
    pub fn new(resources: &Resources) -> Self {
        let mut asset_manager = resources.get_mut::<AssetManager>().unwrap();
        let mut asset_resource = resources.get_mut::<AssetResource>().unwrap();

        log::info!("Loading terrain materials...");

        let terrain_material_paths = vec![
            "materials/terrain/flat_red.pbrmaterial",
            "materials/terrain/flat_green.pbrmaterial",
            "materials/terrain/flat_blue.pbrmaterial",
            "materials/terrain/metal.pbrmaterial",
            "materials/terrain/round-pattern-wallpaper.pbrmaterial",
            "materials/terrain/diamond-inlay-tile.pbrmaterial",
            "materials/terrain/curly_tile.pbrmaterial",
            "materials/terrain/simple_tile.pbrmaterial",
            "materials/terrain/black_plastic.pbrmaterial",
        ];
        let terrain_materials: Vec<_> = terrain_material_paths
            .iter()
            .map(|path| {
                let material_handle = asset_resource.load_asset_path::<PbrMaterialAsset, _>(*path);
                asset_manager
                    .wait_for_asset_to_load(&material_handle, &mut asset_resource, "")
                    .unwrap();
                asset_manager
                    .committed_asset(&material_handle)
                    .unwrap()
                    .clone()
            })
            .collect();

        log::info!("Terrain materials loaded");

        let mut terrain_resource = resources.get_mut::<TerrainResource>().unwrap();
        let w = 4096;
        let terrain = terrain_resource.new_terrain(
            terrain_materials,
            Extent3i::from_min_and_shape(PointN([-w / 2, -w / 2, -1]), PointN([w, w, 1])),
            8.into(),
        );
        let mut objects = HashMap::new();

        let mut building = Array3x1::<CubeVoxel>::fill(
            Extent3i::from_min_and_shape(Point3i::ZERO, PointN([8, 8, 8])),
            0.into(),
        );
        building.fill_extent(
            &Extent3i::from_min_and_shape(Point3i::ZERO, PointN([8, 8, 4])),
            6.into(),
        );
        building.fill_extent(
            &Extent3i::from_min_and_shape(PointN([1, 1, 4]), PointN([6, 6, 3])),
            6.into(),
        );
        building.fill_extent(
            &Extent3i::from_min_and_shape(PointN([1, 1, 7]), PointN([6, 6, 1])),
            5.into(),
        );
        objects.insert(KinObjectType::Building, building);

        let mut tree = Array3x1::<CubeVoxel>::fill(
            Extent3i::from_min_and_shape(PointN([-2, -2, 0]), PointN([5, 5, 9])),
            0.into(),
        );
        tree.fill_extent(
            &Extent3i::from_min_and_shape(Point3i::ZERO, PointN([1, 1, 4])),
            9.into(),
        );
        tree.fill_extent(
            &Extent3i::from_min_and_shape(PointN([-2, -2, 4]), PointN([5, 5, 3])),
            7.into(),
        );
        tree.fill_extent(
            &Extent3i::from_min_and_shape(PointN([-1, -1, 7]), PointN([3, 3, 1])),
            7.into(),
        );
        tree.fill_extent(
            &Extent3i::from_min_and_shape(PointN([0, 0, 8]), PointN([1, 1, 1])),
            7.into(),
        );
        objects.insert(KinObjectType::Tree, tree);

        KinObjectsState {
            terrain,
            objects,
            ui_spawning: false,
            ui_object_type: KinObjectType::Building,
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
                }
            });

        if self.ui_spawning {
            if input.is_mouse_button_just_clicked(MouseButton::LEFT) {
                let cursor_pos = input.mouse_position();
                let cast_result = {
                    let terrain_resource = resources.get::<TerrainResource>().unwrap();
                    let storage = terrain_resource.read();
                    let terrain = storage.get(&self.terrain);
                    camera.ray_cast_terrain(cursor_pos.x as u32, cursor_pos.y as u32, terrain)
                };
                if let Some(result) = cast_result {
                    self.spawn(
                        self.ui_object_type,
                        PointN([result.hit.x(), result.hit.y(), result.hit.z() + 1]),
                        resources,
                        world,
                    );
                }
                self.ui_spawning = false;
            }
        }

        let mut terrain_resource = resources.get_mut::<TerrainResource>().unwrap();
        let mut storage = terrain_resource.write();
        let terrain = storage.get_mut(&self.terrain);
        terrain.update_render_chunks(world, resources);
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
