use crate::{
    assets::gltf::MeshAsset,
    camera::RTSCamera,
    components::{MeshComponent, TransformComponent, VisibilityComponent},
    features::{
        debug3d::Debug3DResource,
        egui::EguiManager,
        mesh::{MeshRenderObject, MeshRenderObjectSet},
    },
    input::{Drag, InputState},
    time::TimeState,
};
use egui::{epaint::Shadow, Button, Color32, Frame, Stroke};
use glam::{Quat, Vec2, Vec3, Vec4};
use itertools::Itertools;
use legion::{IntoQuery, Read, Resources, World, Write};
use rafx::{
    render_feature_extract_job_predule::{ObjectId, RenderObjectHandle, VisibilityRegion},
    render_feature_renderer_prelude::{AssetManager, AssetResource},
    renderer::ViewportsResource,
    visibility::CullModel,
};
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use winit::event::{MouseButton, VirtualKeyCode};

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum DynObjectType {
    Container1,
    Container2,
    BlueIcosphere,
}

#[derive(Clone)]
pub struct DynObjectComponent {
    pub object_type: DynObjectType,
    pub health: f32,
    pub aim: Vec3,
    pub speed: f32,
    pub move_target: Option<Vec3>,
    pub selected: bool,
}

pub struct DynObjectsState {
    meshes: HashMap<DynObjectType, RenderObjectHandle>,
    ui_spawning: bool,
    ui_object_type: DynObjectType,
    pub ui_selected_count: u32,
    pub ui_selected_str: String,
}

impl DynObjectsState {
    pub fn new(resources: &Resources) -> Self {
        let mut asset_manager = resources.get_mut::<AssetManager>().unwrap();
        let mut asset_resource = resources.get_mut::<AssetResource>().unwrap();
        let mut mesh_render_objects = resources.get_mut::<MeshRenderObjectSet>().unwrap();

        let container_1_asset = asset_resource.load_asset_path("blender/storage_container1.glb");
        let container_2_asset = asset_resource.load_asset_path("blender/storage_container2.glb");
        let blue_icosphere_asset =
            asset_resource.load_asset::<MeshAsset>("d5aed900-1e31-4f47-94ba-e356b0b0b8b0".into());

        asset_manager
            .wait_for_asset_to_load(&container_1_asset, &mut asset_resource, "")
            .unwrap();
        asset_manager
            .wait_for_asset_to_load(&container_2_asset, &mut asset_resource, "")
            .unwrap();
        asset_manager
            .wait_for_asset_to_load(&blue_icosphere_asset, &mut asset_resource, "")
            .unwrap();

        let mut meshes = HashMap::new();
        meshes.insert(
            DynObjectType::Container1,
            mesh_render_objects.register_render_object(MeshRenderObject {
                mesh: container_1_asset,
            }),
        );
        meshes.insert(
            DynObjectType::Container2,
            mesh_render_objects.register_render_object(MeshRenderObject {
                mesh: container_2_asset,
            }),
        );
        meshes.insert(
            DynObjectType::BlueIcosphere,
            mesh_render_objects.register_render_object(MeshRenderObject {
                mesh: blue_icosphere_asset,
            }),
        );

        DynObjectsState {
            meshes,
            ui_spawning: false,
            ui_object_type: DynObjectType::Container1,
            ui_selected_count: 0,
            ui_selected_str: "".to_string(),
        }
    }

    pub fn update(&mut self, world: &mut World, resources: &mut Resources) {
        self.add_debug_draw(resources, world);

        let input = resources.get::<InputState>().unwrap();
        let camera = resources.get::<RTSCamera>().unwrap();
        let dt = resources.get::<TimeState>().unwrap().previous_update_dt();

        let mut selecting = false;
        if input.key_pressed.contains(&VirtualKeyCode::N) {
            self.ui_spawning = true;
        }
        let (x0, y0, x1, y1) = if let Drag::End { x0, y0, x1, y1 } = input.drag {
            selecting = !self.ui_spawning;
            let window_size = resources
                .get::<ViewportsResource>()
                .unwrap()
                .main_window_size;
            (
                (x0.min(x1) as f32 / window_size.width as f32) * 2. - 1.,
                (y0.max(y1) as f32 / window_size.height as f32) * -2. + 1.,
                (x0.max(x1) as f32 / window_size.width as f32) * 2. - 1.,
                (y0.min(y1) as f32 / window_size.height as f32) * -2. + 1.,
            )
        } else {
            (0., 0., 0., 0.)
        };
        if selecting {
            self.ui_selected_count = 0;
        }

        let view_proj = camera.view_proj();
        let mut query = <(
            Write<TransformComponent>,
            Read<VisibilityComponent>,
            Write<DynObjectComponent>,
        )>::query();
        query.par_for_each_mut(world, |(transform, visibility, dyn_object)| {
            if let Some(target) = dyn_object.move_target {
                let target_dir = (target - transform.translation).normalize();
                let orig_dir = Vec3::X;
                if (target_dir - orig_dir).length() > 0.001 {
                    transform.rotation = Quat::from_rotation_arc(orig_dir, target_dir);
                }
                if (target_dir - dyn_object.aim).length() > 0.001 {
                    dyn_object.aim =
                        (dyn_object.aim + (target_dir - dyn_object.aim) * dt).normalize();
                }
                const TARGET_SPEED: f32 = 10.; // m/s
                if dyn_object.speed < TARGET_SPEED {
                    dyn_object.speed = (dyn_object.speed + 2. * dt).min(TARGET_SPEED);
                }
                transform.translation += dyn_object.speed * dt * target_dir;
                visibility.visibility_object_handle.set_transform(
                    transform.translation,
                    transform.rotation,
                    transform.scale,
                );
                if (target - transform.translation).length() < 0.1 {
                    dyn_object.move_target = None;
                    dyn_object.speed = 0.;
                }
            }
            if selecting {
                let pos_hom: Vec4 = (transform.translation, 1.).into();
                let pos_view = view_proj * pos_hom;
                let pos_screen = Vec2::new(pos_view.x / pos_view.w, pos_view.y / pos_view.w);
                dyn_object.selected = pos_screen.x > x0
                    && pos_screen.x < x1
                    && pos_screen.y > y0
                    && pos_screen.y < y1;
            }
        });

        if selecting {
            let mut selected = HashMap::<DynObjectType, u32>::new();
            let mut query = <Read<DynObjectComponent>>::query();
            for dyn_object in query.iter(world) {
                if dyn_object.selected {
                    self.ui_selected_count += 1;
                    let entry = selected.entry(dyn_object.object_type);
                    entry.and_modify(|e| *e += 1).or_insert(1);
                }
            }
            let detailed = selected
                .iter()
                .map(|(ty, count)| format!("{:?}: {}", ty, count))
                .join(", ");
            self.ui_selected_str = format!(
                "{} dynamic objects selected ({})",
                self.ui_selected_count, detailed
            );
        }

        {
            let context = resources.get::<EguiManager>().unwrap().context();

            profiling::scope!("egui");
            egui::Window::new("Dynamics")
                .default_pos([10., 40.])
                .default_width(100.)
                .resizable(false)
                .show(&context, |ui| {
                    if self.ui_spawning {
                        ui.label("Click a location on the map to spawn dynamic object");
                    } else {
                        ui.radio_value(
                            &mut self.ui_object_type,
                            DynObjectType::Container1,
                            "Container1",
                        );
                        ui.radio_value(
                            &mut self.ui_object_type,
                            DynObjectType::Container2,
                            "Container2",
                        );
                        ui.radio_value(
                            &mut self.ui_object_type,
                            DynObjectType::BlueIcosphere,
                            "BlueIcosphere",
                        );
                        ui.add_space(10.);
                        if ui.add_sized([100., 30.], Button::new("Spawn")).clicked() {
                            self.ui_spawning = true;
                        }
                    }
                });

            if !self.ui_spawning {
                if let Drag::Dragging { x0, y0, x1, y1 } = input.drag {
                    let s = 1.; //camera.win_scale_factor;
                    let w = (x1 as f32 - x0 as f32).abs() / s;
                    let h = (y1 as f32 - y0 as f32).abs() / s;
                    let x = x0.min(x1) as f32 / s;
                    let y = y0.min(y1) as f32 / s;
                    //if w > 30. && h > 30. {
                    profiling::scope!("egui");
                    egui::Window::new("Selection")
                        .title_bar(false)
                        .frame(Frame {
                            margin: egui::Vec2::ZERO,
                            corner_radius: 4.,
                            shadow: Shadow::default(),
                            fill: Color32::TRANSPARENT,
                            stroke: Stroke {
                                width: 1.,
                                color: Color32::GREEN,
                            },
                        })
                        .fixed_pos([x, y])
                        .fixed_size([w, h])
                        .show(&context, |ui| {
                            ui.add_sized(
                                ui.available_size(),
                                egui::Label::new("")
                                    .small()
                                    .background_color(Color32::TRANSPARENT)
                                    .text_color(Color32::TRANSPARENT),
                            );
                        });
                    //}
                }
            }
        }

        if self.ui_spawning {
            if input.mouse_trigger.contains(&MouseButton::Left) {
                let cursor = camera.ray_cast_terrain(input.cursor_pos.0, input.cursor_pos.1);
                self.spawn(self.ui_object_type, cursor, resources, world);
                self.ui_spawning = false;
            }
        } else if input.mouse_trigger.contains(&MouseButton::Right) {
            let mut first = true;
            let mut target = camera.ray_cast_terrain(input.cursor_pos.0, input.cursor_pos.1);
            let mut query = <(Read<TransformComponent>, Write<DynObjectComponent>)>::query();
            for (transform, dyn_object) in query.iter_mut(world) {
                if dyn_object.selected {
                    if !first {
                        target.x += transform.scale.x;
                    }
                    dyn_object.move_target =
                        Some(Vec3::new(target.x, target.y, transform.translation.z));
                    target.x += transform.scale.x;
                    first = false;
                }
            }
        }
    }

    pub fn spawn(
        &self,
        object_type: DynObjectType,
        position: Vec3,
        resources: &Resources,
        world: &mut World,
    ) {
        // transform component
        const SCALE_MIN: f32 = 0.5;
        const SCALE_MAX: f32 = 2.;
        let position = Vec3::new(position.x, position.y, 0.0);
        let mut rng = thread_rng();
        let rand_scale_z = rng.gen_range(SCALE_MIN, SCALE_MAX);
        let rand_scale_xy = rng.gen_range(SCALE_MIN, SCALE_MAX);
        let offset = rand_scale_z - 1.;
        let transform_component = TransformComponent {
            translation: position + Vec3::new(0., 0., offset),
            scale: Vec3::new(rand_scale_xy, rand_scale_xy, rand_scale_z),
            rotation: Quat::from_rotation_z(rng.gen_range(0., 2. * std::f32::consts::PI)),
        };

        // mesh component
        let mesh_render_object = self.meshes.get(&object_type).unwrap().clone();
        let mesh_component = MeshComponent {
            render_object_handle: mesh_render_object.clone(),
        };

        // dyn object component
        let dyn_object_component = DynObjectComponent {
            object_type,
            health: 1.,
            aim: Vec3::new(1., 0., 0.),
            speed: 0.,
            move_target: None,
            selected: false,
        };

        // entity
        log::info!("Spawn entity {:?} at: {}", object_type, position);
        let entity = world.push((transform_component, mesh_component, dyn_object_component));

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

    pub fn add_debug_draw(&self, resources: &Resources, world: &World) {
        let mut debug_draw = resources.get_mut::<Debug3DResource>().unwrap();

        let normal_col = Vec4::new(1., 0., 0., 1.);
        let selected_col = Vec4::new(0., 1., 0., 1.);

        let mut query = <(Read<TransformComponent>, Read<DynObjectComponent>)>::query();
        for (transform, dyn_object) in query.iter(world) {
            let color = if dyn_object.selected {
                selected_col
            } else {
                normal_col
            };
            let pos = transform.translation;
            let aim = pos + 5. * dyn_object.aim;
            debug_draw.add_line(pos, Vec3::new(pos.x, pos.y, pos.z + 5.), color);
            debug_draw.add_line(pos, aim, color);
            debug_draw.add_cone(aim, pos + 4.7 * dyn_object.aim, 0.1, color, 6);
            if let Some(move_target) = dyn_object.move_target {
                debug_draw.add_line(pos, move_target, color);
            }
        }
    }
}
