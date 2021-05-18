use crate::{
    assets::gltf::MeshAsset,
    camera::RTSCamera,
    components::{MeshComponent, TransformComponent, VisibilityComponent},
    features::{
        debug3d::Debug3DResource,
        mesh::{MeshRenderObject, MeshRenderObjectSet},
    },
    input::{Drag, InputState},
    time::TimeState,
};
use glam::{Quat, Vec2, Vec3, Vec4};
use imgui::im_str;
use itertools::Itertools;
use legion::IntoQuery;
use legion::{Read, Resources, World, Write};
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
pub enum UnitType {
    Container1,
    Container2,
    BlueIcosphere,
}

#[derive(Clone)]
pub struct UnitComponent {
    pub unit_type: UnitType,
    pub health: f32,
    pub aim: Vec3,
    pub speed: f32,
    pub move_target: Option<Vec3>,
    pub selected: bool,
}

pub struct UnitsState {
    meshes: HashMap<UnitType, RenderObjectHandle>,
    ui_spawning: bool,
    ui_unit_type: UnitType,
    pub ui_selected_count: u32,
    pub ui_selected_str: String,
}

impl UnitsState {
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
            UnitType::Container1,
            mesh_render_objects.register_render_object(MeshRenderObject {
                mesh: container_1_asset,
            }),
        );
        meshes.insert(
            UnitType::Container2,
            mesh_render_objects.register_render_object(MeshRenderObject {
                mesh: container_2_asset,
            }),
        );
        meshes.insert(
            UnitType::BlueIcosphere,
            mesh_render_objects.register_render_object(MeshRenderObject {
                mesh: blue_icosphere_asset,
            }),
        );

        UnitsState {
            meshes,
            ui_spawning: false,
            ui_unit_type: UnitType::Container1,
            ui_selected_count: 0,
            ui_selected_str: "".to_string(),
        }
    }

    pub fn update(&mut self, world: &mut World, resources: &mut Resources) {
        self.add_units_debug_draw(resources, world);

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
            Write<UnitComponent>,
        )>::query();
        query.par_for_each_mut(world, |(transform, visibility, unit)| {
            if let Some(target) = unit.move_target {
                let target_dir = (target - transform.translation).normalize();
                let orig_dir = Vec3::X;
                if (target_dir - orig_dir).length() > 0.001 {
                    transform.rotation = Quat::from_rotation_arc(orig_dir, target_dir);
                }
                if (target_dir - unit.aim).length() > 0.001 {
                    unit.aim = (unit.aim + (target_dir - unit.aim) * dt).normalize();
                }
                const TARGET_SPEED: f32 = 10.; // m/s
                if unit.speed < TARGET_SPEED {
                    unit.speed = (unit.speed + 2. * dt).min(TARGET_SPEED);
                }
                transform.translation += unit.speed * dt * target_dir;
                visibility.visibility_object_handle.set_transform(
                    transform.translation,
                    transform.rotation,
                    transform.scale,
                );
                if (target - transform.translation).length() < 0.1 {
                    unit.move_target = None;
                    unit.speed = 0.;
                }
            }
            if selecting {
                let pos_hom: Vec4 = (transform.translation, 1.).into();
                let pos_view = view_proj * pos_hom;
                let pos_screen = Vec2::new(pos_view.x / pos_view.w, pos_view.y / pos_view.w);
                unit.selected = pos_screen.x > x0
                    && pos_screen.x < x1
                    && pos_screen.y > y0
                    && pos_screen.y < y1;
            }
        });

        if selecting {
            let mut selected = HashMap::<UnitType, u32>::new();
            let mut query = <Read<UnitComponent>>::query();
            for unit in query.iter(world) {
                if unit.selected {
                    self.ui_selected_count += 1;
                    let entry = selected.entry(unit.unit_type);
                    entry.and_modify(|e| *e += 1).or_insert(1);
                }
            }
            let detailed = selected
                .iter()
                .map(|(ty, count)| format!("{:?}: {}", ty, count))
                .join(", ");
            self.ui_selected_str =
                format!("{} units selected ({})", self.ui_selected_count, detailed);
        }

        #[cfg(feature = "use-imgui")]
        {
            use crate::features::imgui::ImguiManager;
            profiling::scope!("imgui");
            let imgui_manager = resources.get::<ImguiManager>().unwrap();
            imgui_manager.with_ui(|ui| {
                profiling::scope!("main game menu");

                let game_window = imgui::Window::new(im_str!("Commands"));
                game_window
                    .position([10., 30.], imgui::Condition::FirstUseEver)
                    .always_auto_resize(true)
                    .resizable(false)
                    .build(&ui, || {
                        let group = ui.begin_group();
                        if self.ui_spawning {
                            ui.text_wrapped(im_str!("Click a location on the map to spawn unit"))
                        } else {
                            ui.radio_button(
                                im_str!("Container1"),
                                &mut self.ui_unit_type,
                                UnitType::Container1,
                            );
                            ui.radio_button(
                                im_str!("Container2"),
                                &mut self.ui_unit_type,
                                UnitType::Container2,
                            );
                            ui.radio_button(
                                im_str!("BlueIcosphere"),
                                &mut self.ui_unit_type,
                                UnitType::BlueIcosphere,
                            );
                            if ui.button(im_str!("Spawn new unit"), [100., 30.]) {
                                self.ui_spawning = true;
                            }
                        }
                        group.end(ui);
                    });

                if !self.ui_spawning {
                    if let Drag::Dragging { x0, y0, x1, y1 } = input.drag {
                        let s = camera.win_scale_factor;
                        let w = (x1 as f32 - x0 as f32).abs() / s;
                        let h = (y1 as f32 - y0 as f32).abs() / s;
                        let x = x0.min(x1) as f32 / s;
                        let y = y0.min(y1) as f32 / s;
                        if w > 30. && h > 30. {
                            let selection_window = imgui::Window::new(im_str!("Selection"));
                            selection_window
                                .no_inputs()
                                .no_decoration()
                                .movable(false)
                                .position([x, y], imgui::Condition::Always)
                                .size([w, h], imgui::Condition::Always)
                                .bg_alpha(0.2)
                                .build(&ui, || {});
                        }
                    }
                }
            });
        }

        if self.ui_spawning {
            if input.mouse_trigger.contains(&MouseButton::Left) {
                let cursor = camera.ray_cast_terrain(input.cursor_pos.0, input.cursor_pos.1);
                self.spawn_unit(self.ui_unit_type, cursor, resources, world);
                self.ui_spawning = false;
            }
        } else if input.mouse_trigger.contains(&MouseButton::Right) {
            let mut first = true;
            let mut target = camera.ray_cast_terrain(input.cursor_pos.0, input.cursor_pos.1);
            let mut query = <(Read<TransformComponent>, Write<UnitComponent>)>::query();
            for (transform, unit) in query.iter_mut(world) {
                if unit.selected {
                    if !first {
                        target.x += transform.scale.x;
                    }
                    unit.move_target = Some(Vec3::new(target.x, target.y, transform.translation.z));
                    target.x += transform.scale.x;
                    first = false;
                }
            }
        }
    }

    pub fn spawn_unit(
        &self,
        unit_type: UnitType,
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
        let mesh_render_object = self.meshes.get(&unit_type).unwrap().clone();
        let mesh_component = MeshComponent {
            render_object_handle: mesh_render_object.clone(),
        };

        // unit component
        let unit_component = UnitComponent {
            unit_type,
            health: 1.,
            aim: Vec3::new(1., 0., 0.),
            speed: 0.,
            move_target: None,
            selected: false,
        };

        // entity
        log::info!("Spawn entity {:?} at: {}", unit_type, position);
        let entity = world.push((transform_component, mesh_component, unit_component));

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

    pub fn add_units_debug_draw(&self, resources: &Resources, world: &World) {
        let mut debug_draw = resources.get_mut::<Debug3DResource>().unwrap();

        let normal_col = Vec4::new(1., 0., 0., 1.);
        let selected_col = Vec4::new(0., 1., 0., 1.);

        let mut query = <(Read<TransformComponent>, Read<UnitComponent>)>::query();
        for (transform, unit) in query.iter(world) {
            let color = if unit.selected {
                selected_col
            } else {
                normal_col
            };
            let pos = transform.translation;
            let aim = pos + 5. * unit.aim;
            debug_draw.add_line(pos, Vec3::new(pos.x, pos.y, pos.z + 5.), color);
            debug_draw.add_line(pos, aim, color);
            debug_draw.add_cone(aim, pos + 4.7 * unit.aim, 0.1, color, 6);
            if let Some(move_target) = unit.move_target {
                debug_draw.add_line(pos, move_target, color);
            }
        }
    }
}
