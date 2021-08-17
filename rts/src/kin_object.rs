use crate::{
    assets::terrain::TerrainConfigAsset,
    camera::RTSCamera,
    features::dyn_mesh::{DynMeshRenderObjectSet, DynMeshResource},
    input::{InputResource, MouseButton},
    terrain::{TerrainHandle, TerrainResource},
};
use building_blocks::core::prelude::*;
use egui::Button;
use glam::{Quat, Vec3};
use legion::{Resources, World};
use rafx::{
    assets::{distill_impl::AssetResource, AssetManager},
    render_feature_extract_job_predule::{ObjectId, RenderObjectHandle, VisibilityRegion},
    visibility::CullModel,
};
use rafx_plugins::{
    components::{MeshComponent, TransformComponent, VisibilityComponent},
    features::{egui::EguiContextResource, mesh::MeshRenderObjectSet},
};
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
    meshes: HashMap<KinObjectType, RenderObjectHandle>,
    ui_spawning: bool,
    ui_object_type: KinObjectType,
}

impl KinObjectsState {
    pub fn new(resources: &Resources) -> Self {
        let mut asset_manager = resources.get_mut::<AssetManager>().unwrap();
        let mut asset_resource = resources.get_mut::<AssetResource>().unwrap();
        let terrain_config_asset_handle = asset_resource
            .load_asset_path::<TerrainConfigAsset, _>("materials/terrain_materials.terrainconfig");
        let terrain_config_asset = {
            asset_manager
                .wait_for_asset_to_load(&terrain_config_asset_handle, &mut asset_resource, "")
                .unwrap();
            asset_manager
                .committed_asset(&terrain_config_asset_handle)
                .unwrap()
                .clone()
        };
        let mut terrain_resource = resources.get_mut::<TerrainResource>().unwrap();
        let terrain = terrain_resource.new_terrain(
            terrain_config_asset,
            Extent3i::from_min_and_shape(Point3i::ZERO, Point3i::fill(256)),
            1.into(),
        );
        let meshes = HashMap::new();
        KinObjectsState {
            terrain,
            meshes,
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

        let mut dyn_mesh_resource = resources.get_mut::<DynMeshResource>().unwrap();
        let mut dyn_mesh_render_objects = resources.get_mut::<DynMeshRenderObjectSet>().unwrap();
        let visibility_region = resources.get::<VisibilityRegion>().unwrap();
        let mut terrain_resource = resources.get_mut::<TerrainResource>().unwrap();
        let mut storage = terrain_resource.write();
        let terrain = storage.get_mut(&self.terrain);
        terrain.update_render_chunks(
            &mut dyn_mesh_resource,
            &mut dyn_mesh_render_objects,
            &visibility_region,
        );
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

        // mesh component
        let mesh_render_object = self.meshes.get(&object_type).unwrap().clone();
        let mesh_component = MeshComponent {
            render_object_handle: mesh_render_object.clone(),
        };

        // kin object component
        let kin_object_component = KinObjectComponent {
            object_type,
            health: 1.,
            selected: false,
        };

        // entity
        log::info!("Spawn entity {:?} at: {}", object_type, position);
        let entity = world.push((transform_component, mesh_component, kin_object_component));

        // visibility component
        let asset_manager = resources.get::<AssetManager>().unwrap();
        let visibility_region = resources.get::<VisibilityRegion>().unwrap();
        let mesh_render_objects = resources.get::<MeshRenderObjectSet>().unwrap();
        let mesh_render_objects = mesh_render_objects.read();
        let asset_handle = &mesh_render_objects.get(&mesh_render_object).mesh;
        let mut entry = world.entry(entity).unwrap();
        entry.add_component(VisibilityComponent {
            visibility_object_handle: {
                let handle = visibility_region.register_dynamic_object(
                    ObjectId::from(entity),
                    CullModel::VisibleBounds(
                        asset_manager
                            .committed_asset(&asset_handle)
                            .unwrap()
                            .inner
                            .asset_data
                            .visible_bounds,
                    ),
                );
                handle.set_transform(
                    transform_component.translation,
                    transform_component.rotation,
                    transform_component.scale,
                );
                handle.add_render_object(&mesh_render_object);
                handle
            },
        });
    }
}
