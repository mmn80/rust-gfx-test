use std::{collections::HashMap, fmt::Display};

use egui::{epaint::Shadow, Color32, Frame, Stroke};
use glam::{Quat, Vec2, Vec3, Vec4};
use legion::{IntoQuery, Read, Resources, World, Write};
use rafx::{
    assets::{distill_impl::AssetResource, AssetManager},
    framework::{
        render_features::RenderObjectHandle,
        visibility::{ObjectId, VisibilityRegion},
    },
    renderer::ViewportsResource,
    visibility::CullModel,
};
use rafx_plugins::{
    assets::mesh::MeshAsset,
    components::{MeshComponent, TransformComponent, VisibilityComponent},
    features::{
        debug3d::Debug3DResource,
        egui::EguiContextResource,
        mesh::{MeshRenderObject, MeshRenderObjectSet},
    },
};
use rand::{thread_rng, Rng};

use crate::{
    camera::RTSCamera,
    env::simulation::Simulation,
    input::{InputResource, MouseButton, MouseDragState},
    time::TimeState,
    ui::{SpawnMode, UiState},
};

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum UnitType {
    Container1,
    Container2,
    BlueIcosphere,
}

impl Display for UnitType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            UnitType::Container1 => write!(f, "Container 1"),
            UnitType::Container2 => write!(f, "Container 2"),
            UnitType::BlueIcosphere => write!(f, "Blue icosphere"),
        }
    }
}

#[derive(Clone)]
pub struct UnitComponent {
    pub object_type: UnitType,
    pub health: f32,
    pub aim: Vec3,
    pub speed: f32,
    pub move_target: Option<Vec3>,
    pub selected: bool,
}

pub struct UnitUiState {
    pub spawning: bool,
    pub spawn_mode: SpawnMode,
    pub object_type: UnitType,
    pub selecting: bool,
    pub selected_count: u32,
    pub selected: HashMap<UnitType, u32>,
}

impl Default for UnitUiState {
    fn default() -> Self {
        Self {
            spawning: false,
            spawn_mode: SpawnMode::OneShot,
            object_type: UnitType::Container1,
            selecting: false,
            selected_count: 0,
            selected: Default::default(),
        }
    }
}

pub struct UnitsState {
    meshes: HashMap<UnitType, RenderObjectHandle>,
}

impl UnitsState {
    pub fn new(resources: &Resources) -> Self {
        let mut asset_manager = resources.get_mut::<AssetManager>().unwrap();
        let mut asset_resource = resources.get_mut::<AssetResource>().unwrap();
        let mut mesh_render_objects = resources.get_mut::<MeshRenderObjectSet>().unwrap();

        log::info!("Loading units meshes...");

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

        log::info!("Units meshes loaded");

        UnitsState { meshes }
    }

    pub fn update_ui(
        &mut self,
        simulation: &mut Simulation,
        resources: &mut Resources,
        ui_state: &mut UiState,
        ui: &mut egui::Ui,
    ) {
        let universe = simulation.universe();

        self.add_debug_draw(resources, &universe.world);

        let input = resources.get::<InputResource>().unwrap();
        let camera = resources.get::<RTSCamera>().unwrap();

        ui_state.unit.selecting = false;
        if let Some(MouseDragState { .. }) = input.mouse_drag_just_finished(MouseButton::LEFT) {
            ui_state.unit.selecting = !ui_state.unit.spawning
                && !ui_state.env.tile_spawn.active
                && !ui_state.env.terrain_edit.active;
        }

        if ui_state.unit.spawning {
            egui::CollapsingHeader::new("Spawn unit")
                .default_open(true)
                .show(ui, |ui| {
                    ui_state.unit.spawn_mode.ui(ui, &mut ui_state.unit.spawning);
                    ui.label("Click a location on the map to spawn unit");
                });
        } else if !ui_state.env.tile_spawn.active {
            egui::CollapsingHeader::new("Spawn unit")
                .default_open(true)
                .show(ui, |ui| {
                    ui_state.unit.spawn_mode.ui(ui, &mut ui_state.unit.spawning);
                    ui.horizontal_wrapped(|ui| {
                        for (obj, _) in &self.meshes {
                            if ui.selectable_label(false, format!("{}", obj)).clicked() {
                                ui_state.unit.object_type = *obj;
                                ui_state.unit.spawning = true;
                            }
                        }
                    });
                });
        }

        if ui_state.unit.selected_count > 0 {
            egui::CollapsingHeader::new("Object selection")
                .default_open(true)
                .show(ui, |ui| {
                    ui.label(format!("{} units selected", ui_state.unit.selected_count));
                    for (ty, count) in &ui_state.unit.selected {
                        ui.label(format!("- {:?}: {}", ty, count));
                    }
                });
        }

        if !ui_state.unit.spawning
            && !ui_state.env.tile_spawn.active
            && !ui_state.env.terrain_edit.active
        {
            if let Some(MouseDragState {
                begin_position: p0,
                end_position: p1,
                ..
            }) = input.mouse_drag_in_progress(MouseButton::LEFT)
            {
                let (p0, p1) = {
                    let scale_factor = ui.ctx().pixels_per_point();
                    (p0 / scale_factor, p1 / scale_factor)
                };
                let w = (p1.x as f32 - p0.x as f32).abs();
                let h = (p1.y as f32 - p0.y as f32).abs();
                let x = p0.x.min(p1.x) as f32;
                let y = p0.y.min(p1.y) as f32;
                //if w > 30. && h > 30. {

                let context = resources.get::<EguiContextResource>().unwrap().context();
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

        if ui_state.unit.spawning {
            if input.is_mouse_just_down(MouseButton::LEFT) {
                let cursor_pos = input.mouse_position();
                let cast_result = camera.ray_cast_terrain(
                    cursor_pos.x as u32,
                    cursor_pos.y as u32,
                    universe,
                    ui_state,
                );
                if let Some(result) = cast_result {
                    let p = result.hit;
                    self.spawn(
                        ui_state.unit.object_type,
                        Vec3::new(p.x() as f32, p.y() as f32, p.z() as f32 + 1.),
                        resources,
                        &mut universe.world,
                        &universe.visibility_region,
                    );
                }
                if ui_state.unit.spawn_mode == SpawnMode::OneShot {
                    ui_state.unit.spawning = false;
                }
            }
        } else if input.is_mouse_just_down(MouseButton::RIGHT) {
            let mut first = true;
            let cursor_pos = input.mouse_position();
            let cast_result = camera.ray_cast_terrain(
                cursor_pos.x as u32,
                cursor_pos.y as u32,
                universe,
                ui_state,
            );
            if let Some(result) = cast_result {
                let p = result.hit;
                let mut target = Vec3::new(p.x() as f32, p.y() as f32, p.z() as f32 + 2.);
                let mut query = <(Read<TransformComponent>, Write<UnitComponent>)>::query();
                for (transform, dyn_object) in query.iter_mut(&mut universe.world) {
                    if dyn_object.selected {
                        if !first {
                            target.x += transform.scale.x;
                        }
                        dyn_object.move_target = Some(target);
                        target.x += transform.scale.x;
                        first = false;
                    }
                }
            }
        }
    }

    #[profiling::function]
    pub fn update(
        &mut self,
        simulation: &mut Simulation,
        resources: &mut Resources,
        ui_state: &mut UiState,
    ) {
        let camera = resources.get::<RTSCamera>().unwrap();
        let view_proj = camera.view_proj();
        let dt = resources.get::<TimeState>().unwrap().previous_update_dt();
        let input = resources.get::<InputResource>().unwrap();
        let universe = simulation.universe();

        let (x0, y0, x1, y1) = if let Some(MouseDragState {
            begin_position: p0,
            end_position: p1,
            ..
        }) = input.mouse_drag_just_finished(MouseButton::LEFT)
        {
            let window_size = resources
                .get::<ViewportsResource>()
                .unwrap()
                .main_window_size;
            (
                (p0.x.min(p1.x) / window_size.width as f32) * 2. - 1.,
                (p0.y.max(p1.y) / window_size.height as f32) * -2. + 1.,
                (p0.x.max(p1.x) / window_size.width as f32) * 2. - 1.,
                (p0.y.min(p1.y) / window_size.height as f32) * -2. + 1.,
            )
        } else {
            (0., 0., 0., 0.)
        };

        let mut query = <(
            Write<TransformComponent>,
            Read<VisibilityComponent>,
            Write<UnitComponent>,
        )>::query();
        query.par_for_each_mut(&mut universe.world, |(transform, visibility, unit)| {
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
            if ui_state.unit.selecting {
                let pos_hom: Vec4 = (transform.translation, 1.).into();
                let pos_view = view_proj * pos_hom;
                let pos_screen = Vec2::new(pos_view.x / pos_view.w, pos_view.y / pos_view.w);
                unit.selected = pos_screen.x > x0
                    && pos_screen.x < x1
                    && pos_screen.y > y0
                    && pos_screen.y < y1;
            }
        });

        if ui_state.unit.selecting {
            ui_state.unit.selected_count = 0;
            ui_state.unit.selected.clear();
            let mut query = <Read<UnitComponent>>::query();
            for dyn_object in query.iter(&universe.world) {
                if dyn_object.selected {
                    ui_state.unit.selected_count += 1;
                    let entry = ui_state.unit.selected.entry(dyn_object.object_type);
                    entry.and_modify(|e| *e += 1).or_insert(1);
                }
            }
        }
    }

    pub fn spawn(
        &self,
        unit_type: UnitType,
        position: Vec3,
        resources: &Resources,
        world: &mut World,
        visibility_region: &VisibilityRegion,
    ) {
        // transform component
        const SCALE_MIN: f32 = 0.5;
        const SCALE_MAX: f32 = 2.;
        let position = Vec3::new(position.x, position.y, position.z + 1.);
        let mut rng = thread_rng();
        let rand_scale_xy = rng.gen_range(SCALE_MIN..SCALE_MAX);
        let transform_component = TransformComponent {
            translation: position,
            scale: Vec3::new(rand_scale_xy, rand_scale_xy, 1.),
            rotation: Quat::from_rotation_z(rng.gen_range(0.0..2.0 * std::f32::consts::PI)),
        };

        // mesh component
        let mesh_render_object = self.meshes.get(&unit_type).unwrap().clone();
        let mesh_component = MeshComponent {
            render_object_handle: mesh_render_object.clone(),
        };

        // unit component
        let unit_component = UnitComponent {
            object_type: unit_type,
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

        let mut query = <(Read<TransformComponent>, Read<UnitComponent>)>::query();
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
