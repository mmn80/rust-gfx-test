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
    terrain: TerrainHandle,
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
            "materials/terrain_flat_red.pbrmaterial",
            "materials/terrain_flat_green.pbrmaterial",
            "materials/terrain_flat_blue.pbrmaterial",
            "materials/terrain_metal.pbrmaterial",
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
            1.into(),
        );
        let mut objects = HashMap::new();

        let building = Array3x1::<CubeVoxel>::fill(
            Extent3i::from_min_and_shape(Point3i::ZERO, PointN([10, 10, 10])),
            4.into(),
        );
        objects.insert(KinObjectType::Building, building);

        let mut tree = Array3x1::<CubeVoxel>::fill(
            Extent3i::from_min_and_shape(PointN([-2, -2, 0]), PointN([5, 5, 15])),
            0.into(),
        );
        tree.fill_extent(
            &Extent3i::from_min_and_shape(Point3i::ZERO, PointN([1, 1, 10])),
            3.into(),
        );
        tree.fill_extent(
            &Extent3i::from_min_and_shape(PointN([-2, -2, 10]), PointN([5, 5, 5])),
            2.into(),
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
                let cursor = camera.ray_cast_terrain(cursor_pos.x as u32, cursor_pos.y as u32);
                self.spawn(self.ui_object_type, cursor, resources, world);
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
        position: Vec3,
        resources: &Resources,
        world: &mut World,
    ) {
        // transform component
        let position = Vec3::new(position.x, position.y, 0.0);
        let transform_component = TransformComponent {
            translation: position,
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
        log::info!("Spawn entity {:?} at: {}", object_type, position);
        let _entity = world.push((transform_component, kin_object_component));

        // update voxels
        let mut terrain_resource = resources.get_mut::<TerrainResource>().unwrap();
        let mut storage = terrain_resource.write();
        let terrain = storage.get_mut(&self.terrain);

        let mut object = self.objects.get(&object_type).unwrap().clone();
        object.set_minimum(PointN([
            position.x as i32,
            position.y as i32,
            position.z as i32,
        ]));
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
