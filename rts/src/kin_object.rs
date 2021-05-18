use crate::{
    camera::RTSCamera,
    components::{MeshComponent, TransformComponent, VisibilityComponent},
    features::mesh::MeshRenderObjectSet,
    input::InputState,
};
use glam::{Quat, Vec3};
use imgui::im_str;
use legion::{Resources, World};
use rafx::{
    render_feature_extract_job_predule::{ObjectId, RenderObjectHandle, VisibilityRegion},
    render_feature_renderer_prelude::AssetManager,
    visibility::CullModel,
};
use std::collections::HashMap;
use winit::event::MouseButton;

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
    meshes: HashMap<KinObjectType, RenderObjectHandle>,
    ui_spawning: bool,
    ui_object_type: KinObjectType,
}

impl KinObjectsState {
    pub fn new(resources: &Resources) -> Self {
        let mut asset_manager = resources.get_mut::<AssetManager>().unwrap();
        let mut mesh_render_objects = resources.get_mut::<MeshRenderObjectSet>().unwrap();
        let mut meshes = HashMap::new();
        KinObjectsState {
            meshes,
            ui_spawning: false,
            ui_object_type: KinObjectType::Building,
        }
    }

    pub fn update(&mut self, world: &mut World, resources: &mut Resources) {
        let input = resources.get::<InputState>().unwrap();
        let camera = resources.get::<RTSCamera>().unwrap();

        #[cfg(feature = "use-imgui")]
        {
            use crate::features::imgui::ImguiManager;
            profiling::scope!("imgui");
            let imgui_manager = resources.get::<ImguiManager>().unwrap();
            imgui_manager.with_ui(|ui| {
                profiling::scope!("main game menu");

                let game_window = imgui::Window::new(im_str!("Kinematics"));
                game_window
                    .position([150., 30.], imgui::Condition::FirstUseEver)
                    .always_auto_resize(true)
                    .resizable(false)
                    .build(&ui, || {
                        let group = ui.begin_group();
                        if self.ui_spawning {
                            ui.text_wrapped(im_str!(
                                "Click a location on the map to spawn kinematic object"
                            ))
                        } else {
                            ui.radio_button(
                                im_str!("Building"),
                                &mut self.ui_object_type,
                                KinObjectType::Building,
                            );
                            ui.radio_button(
                                im_str!("Tree"),
                                &mut self.ui_object_type,
                                KinObjectType::Tree,
                            );
                            if ui.button(im_str!("Spawn"), [100., 30.]) {
                                self.ui_spawning = true;
                            }
                        }
                        group.end(ui);
                    });
            });
        }
        if self.ui_spawning {
            if input.mouse_trigger.contains(&MouseButton::Left) {
                let cursor = camera.ray_cast_terrain(input.cursor_pos.0, input.cursor_pos.1);
                self.spawn(self.ui_object_type, cursor, resources, world);
                self.ui_spawning = false;
            }
        }
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
