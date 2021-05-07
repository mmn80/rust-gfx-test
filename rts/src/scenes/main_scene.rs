use super::{Scene, SceneManagerAction};
use crate::components::{SpotLightComponent, VisibilityComponent};
use crate::features::mesh::{MeshRenderNode, MeshRenderNodeSet};
use crate::features::text::TextResource;
use crate::time::TimeState;
use crate::RenderOptions;
use crate::{assets::font::FontAsset, input::InputState};
use crate::{assets::gltf::MeshAsset, features::mesh::MeshRenderNodeHandle};
use crate::{camera::RTSCamera, components::UnitType};
use crate::{
    components::{
        DirectionalLightComponent, MeshComponent, PointLightComponent, TransformComponent,
        UnitComponent,
    },
    input::Drag,
};
use distill::loader::handle::Handle;
use glam::{Quat, Vec2, Vec3};
use imgui::im_str;
use legion::IntoQuery;
use legion::{Read, Resources, World, Write};
use rafx::assets::distill_impl::AssetResource;
use rafx::assets::AssetManager;
use rafx::renderer::ViewportsResource;
use rafx::visibility::{CullModel, EntityId, ViewFrustumArc, VisibilityRegion};
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use winit::event::{MouseButton, VirtualKeyCode};

pub(super) struct MainScene {
    main_view_frustum: ViewFrustumArc,
    font: Handle<FontAsset>,
    meshes: HashMap<UnitType, MeshRenderNodeHandle>,
    ui_spawning: bool,
    ui_unit_type: UnitType,
}

impl MainScene {
    pub(super) fn new(world: &mut World, resources: &Resources) -> Self {
        let mut asset_manager = resources.get_mut::<AssetManager>().unwrap();
        let mut asset_resource = resources.get_mut::<AssetResource>().unwrap();

        let mut render_options = resources.get_mut::<RenderOptions>().unwrap();
        *render_options = RenderOptions::default_3d();

        let mut mesh_render_nodes = resources.get_mut::<MeshRenderNodeSet>().unwrap();

        let visibility_region = resources.get::<VisibilityRegion>().unwrap();

        let font = asset_resource.load_asset_path::<FontAsset, _>("fonts/mplus-1p-regular.ttf");

        let floor_mesh_asset =
            asset_resource.load_asset_path::<MeshAsset, _>("blender/cement_floor.glb");
        let container_1_asset = asset_resource.load_asset_path("blender/storage_container1.glb");
        let container_2_asset = asset_resource.load_asset_path("blender/storage_container2.glb");
        let blue_icosphere_asset =
            asset_resource.load_asset::<MeshAsset>("d5aed900-1e31-4f47-94ba-e356b0b0b8b0".into());

        asset_manager
            .wait_for_asset_to_load(&floor_mesh_asset, &mut asset_resource, "")
            .unwrap();
        asset_manager
            .wait_for_asset_to_load(&container_1_asset, &mut asset_resource, "")
            .unwrap();
        asset_manager
            .wait_for_asset_to_load(&container_2_asset, &mut asset_resource, "")
            .unwrap();
        asset_manager
            .wait_for_asset_to_load(&blue_icosphere_asset, &mut asset_resource, "")
            .unwrap();

        const FLOOR_SIZE: f32 = 48.;
        const FLOOR_NUM: i32 = 10;

        //
        // Add a floor
        //
        {
            let floor_mesh = mesh_render_nodes.register_mesh(MeshRenderNode {
                mesh: floor_mesh_asset.clone(),
            });

            for x in -FLOOR_NUM..FLOOR_NUM {
                for y in -FLOOR_NUM..FLOOR_NUM {
                    let position = Vec3::new(x as f32 * FLOOR_SIZE, y as f32 * FLOOR_SIZE, -1.);
                    let transform_component = TransformComponent {
                        translation: position,
                        ..Default::default()
                    };

                    let mesh_component = MeshComponent {
                        render_node: floor_mesh.clone(),
                    };

                    let entity = world.push((transform_component, mesh_component));
                    let mut entry = world.entry(entity).unwrap();
                    entry.add_component(VisibilityComponent {
                        handle: {
                            let handle = visibility_region.register_static_object(
                                EntityId::from(entity),
                                CullModel::VisibleBounds(
                                    asset_manager
                                        .committed_asset(&floor_mesh_asset)
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
                            handle.add_feature(floor_mesh.as_raw_generic_handle());
                            handle
                        },
                    });
                }
            }
        }

        //
        // Add some meshes
        //

        let mut meshes = HashMap::new();
        meshes.insert(
            UnitType::Container1,
            mesh_render_nodes.register_mesh(MeshRenderNode {
                mesh: container_1_asset,
            }),
        );
        meshes.insert(
            UnitType::Container2,
            mesh_render_nodes.register_mesh(MeshRenderNode {
                mesh: container_2_asset,
            }),
        );
        meshes.insert(
            UnitType::BlueIcosphere,
            mesh_render_nodes.register_mesh(MeshRenderNode {
                mesh: blue_icosphere_asset,
            }),
        );

        //
        // POINT LIGHT
        //
        let view_frustums = [
            visibility_region.register_view_frustum(),
            visibility_region.register_view_frustum(),
            visibility_region.register_view_frustum(),
            visibility_region.register_view_frustum(),
            visibility_region.register_view_frustum(),
            visibility_region.register_view_frustum(),
        ];
        super::add_point_light(
            resources,
            world,
            //Vec3::new(-3.0, 3.0, 2.0),
            Vec3::new(5.0, 5.0, 2.0),
            PointLightComponent {
                color: [0.0, 1.0, 0.0, 1.0].into(),
                intensity: 50.0,
                range: 25.0,
                view_frustums,
            },
        );

        //
        // DIRECTIONAL LIGHT
        //
        let light_from = Vec3::new(-5.0, 5.0, 5.0);
        let light_to = Vec3::ZERO;
        let light_direction = (light_to - light_from).normalize();
        super::add_directional_light(
            resources,
            world,
            DirectionalLightComponent {
                direction: light_direction,
                intensity: 5.0,
                color: [1.0, 1.0, 1.0, 1.0].into(),
                view_frustum: visibility_region.register_view_frustum(),
            },
        );

        //
        // SPOT LIGHT
        //
        let light_from = Vec3::new(-3.0, -3.0, 5.0);
        let light_to = Vec3::ZERO;
        let light_direction = (light_to - light_from).normalize();
        super::add_spot_light(
            resources,
            world,
            light_from,
            SpotLightComponent {
                direction: light_direction,
                spotlight_half_angle: 40.0 * (std::f32::consts::PI / 180.0),
                range: 12.0,
                color: [1.0, 0.0, 0.0, 1.0].into(),
                intensity: 500.0,
                view_frustum: visibility_region.register_view_frustum(),
            },
        );

        let main_view_frustum = visibility_region.register_view_frustum();

        MainScene {
            main_view_frustum,
            font,
            meshes,
            ui_spawning: false,
            ui_unit_type: UnitType::Container1,
        }
    }
}

impl super::GameScene for MainScene {
    fn update(&mut self, world: &mut World, resources: &mut Resources) -> SceneManagerAction {
        //super::add_light_debug_draw(&resources, &world);
        super::add_units_debug_draw(&resources, &world);

        {
            let input = resources.get::<InputState>().unwrap();
            if input.key_pressed.contains(&VirtualKeyCode::N) {
                self.ui_spawning = true;
            }
            let time_state = resources.get::<TimeState>().unwrap();
            let mut viewports_resource = resources.get_mut::<ViewportsResource>().unwrap();
            let render_options = resources.get::<RenderOptions>().unwrap();
            let mut camera = resources.get_mut::<RTSCamera>().unwrap();

            camera.update(
                &*time_state,
                &*render_options,
                &mut self.main_view_frustum,
                &mut *viewports_resource,
                &input,
            );
        }

        {
            let time_state = resources.get::<TimeState>().unwrap();
            let mut query = <Write<DirectionalLightComponent>>::query();
            for mut light in query.iter_mut(world) {
                const LIGHT_XY_DISTANCE: f32 = 50.0;
                const LIGHT_Z: f32 = 50.0;
                const LIGHT_ROTATE_SPEED: f32 = 0.2;
                const LIGHT_LOOP_OFFSET: f32 = 2.0;
                let loop_time = time_state.total_time().as_secs_f32();
                let light_from = Vec3::new(
                    LIGHT_XY_DISTANCE
                        * f32::cos(LIGHT_ROTATE_SPEED * loop_time + LIGHT_LOOP_OFFSET),
                    LIGHT_XY_DISTANCE
                        * f32::sin(LIGHT_ROTATE_SPEED * loop_time + LIGHT_LOOP_OFFSET),
                    LIGHT_Z,
                    //LIGHT_Z// * f32::sin(LIGHT_ROTATE_SPEED * loop_time + LIGHT_LOOP_OFFSET).abs(),
                    //0.2
                    //2.0
                );
                let light_to = Vec3::default();

                light.direction = (light_to - light_from).normalize();
            }
        }

        {
            let time_state = resources.get::<TimeState>().unwrap();
            let mut query = <(Write<TransformComponent>, Read<PointLightComponent>)>::query();
            for (transform, _light) in query.iter_mut(world) {
                const LIGHT_XY_DISTANCE: f32 = 6.0;
                const LIGHT_Z: f32 = 3.5;
                const LIGHT_ROTATE_SPEED: f32 = 0.5;
                const LIGHT_LOOP_OFFSET: f32 = 2.0;
                let loop_time = time_state.total_time().as_secs_f32();
                let light_from = Vec3::new(
                    LIGHT_XY_DISTANCE
                        * f32::cos(LIGHT_ROTATE_SPEED * loop_time + LIGHT_LOOP_OFFSET),
                    LIGHT_XY_DISTANCE
                        * f32::sin(LIGHT_ROTATE_SPEED * loop_time + LIGHT_LOOP_OFFSET),
                    LIGHT_Z,
                    //LIGHT_Z// * f32::sin(LIGHT_ROTATE_SPEED * loop_time + LIGHT_LOOP_OFFSET).abs(),
                    //0.2
                    //2.0
                );
                transform.translation = light_from;
            }
        }

        {
            let input = resources.get::<InputState>().unwrap();
            let mut selecting = false;
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
            let view_proj = resources.get::<RTSCamera>().unwrap().view_proj();
            let time_state = resources.get::<TimeState>().unwrap();
            let mut query = <(Write<TransformComponent>, Write<UnitComponent>)>::query();
            query.par_for_each_mut(world, |(transform, unit)| {
                if let Some(target) = unit.move_target {
                    let dt = time_state.previous_update_dt();
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
                    if (target - transform.translation).length() < 0.1 {
                        unit.move_target = None;
                        unit.speed = 0.;
                    }
                }
                if selecting {
                    let pos_hom: glam::Vec4 = (transform.translation, 1.).into();
                    let pos_view = view_proj * pos_hom;
                    let pos_screen = Vec2::new(pos_view.x / pos_view.w, pos_view.y / pos_view.w);
                    unit.selected = pos_screen.x > x0
                        && pos_screen.x < x1
                        && pos_screen.y > y0
                        && pos_screen.y < y1;
                }
            });
            if selecting {
                let mut selected = 0;
                let mut s = String::new();
                let mut query = <(Read<TransformComponent>, Read<UnitComponent>)>::query();
                for (transform, unit) in query.iter(world) {
                    if unit.selected {
                        selected += 1;
                        s.push_str(format!("{}, ", transform.translation).as_str());
                    }
                }
                log::info!("{} selected: {}", selected, s);
            }
        }

        #[cfg(feature = "use-imgui")]
        {
            let input = resources.get::<InputState>().unwrap();
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
                        let w = (x1 as f32 - x0 as f32).abs();
                        let h = (y1 as f32 - y0 as f32).abs();
                        let x = x0.min(x1) as f32;
                        let y = y0.min(y1) as f32;
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

        {
            let viewports_resource = resources.get::<ViewportsResource>().unwrap();
            let mut text_resource = resources.get_mut::<TextResource>().unwrap();
            let camera = resources.get::<RTSCamera>().unwrap();
            text_resource.add_text(
                format!("camera: {:.2}m", camera.look_at_dist),
                Vec3::new(
                    10.0,
                    viewports_resource.main_window_size.height as f32 - 30.,
                    0.0,
                ),
                &self.font,
                20.0,
                glam::Vec4::new(1.0, 1.0, 1.0, 1.0),
            );
        }

        {
            let input = resources.get::<InputState>().unwrap();
            if self.ui_spawning {
                if input.mouse_trigger.contains(&MouseButton::Left) {
                    let camera = resources.get::<RTSCamera>().unwrap();
                    let cursor = camera.ray_cast_terrain(input.cursor_pos.0, input.cursor_pos.1);
                    self.spawn_unit(self.ui_unit_type, cursor, resources, world);
                    self.ui_spawning = false;
                }
            } else if input.mouse_trigger.contains(&MouseButton::Right) {
                let camera = resources.get::<RTSCamera>().unwrap();
                let mut first = true;
                let mut target = camera.ray_cast_terrain(input.cursor_pos.0, input.cursor_pos.1);
                let mut query = <(Read<TransformComponent>, Write<UnitComponent>)>::query();
                for (transform, unit) in query.iter_mut(world) {
                    if unit.selected {
                        if !first {
                            target.x += transform.scale.x;
                        }
                        unit.move_target =
                            Some(Vec3::new(target.x, target.y, transform.translation.z));
                        target.x += transform.scale.x;
                        first = false;
                    }
                }
            }
            if input.key_trigger.contains(&VirtualKeyCode::Escape) {
                SceneManagerAction::Scene(Scene::Menu)
            } else {
                SceneManagerAction::None
            }
        }
    }

    fn cleanup(&mut self, _world: &mut World, _resources: &Resources) {}
}

impl MainScene {
    fn spawn_unit(
        &mut self,
        unit_type: UnitType,
        position: Vec3,
        resources: &Resources,
        world: &mut World,
    ) {
        const SCALE_MIN: f32 = 0.5;
        const SCALE_MAX: f32 = 2.;
        let asset_manager = resources.get::<AssetManager>().unwrap();
        let visibility_region = resources.get::<VisibilityRegion>().unwrap();
        let mesh_render_nodes = resources.get::<MeshRenderNodeSet>().unwrap();
        let mut rng = thread_rng();
        let position = Vec3::new(position.x, position.y, 0.0);
        let mesh_render_node = self.meshes.get(&unit_type).unwrap().clone();
        let asset_handle = &mesh_render_nodes.get(&mesh_render_node).unwrap().mesh;
        let rand_scale_z = rng.gen_range(SCALE_MIN, SCALE_MAX);
        let rand_scale_xy = rng.gen_range(SCALE_MIN, SCALE_MAX);
        let offset = rand_scale_z - 1.;
        let transform_component = TransformComponent {
            translation: position + Vec3::new(0., 0., offset),
            scale: Vec3::new(rand_scale_xy, rand_scale_xy, rand_scale_z),
            rotation: Quat::from_rotation_z(rng.gen_range(0., 2. * std::f32::consts::PI)),
        };
        log::info!("Spawn entity {:?} at: {}", unit_type, position);
        let unit_component = UnitComponent {
            unit_type,
            health: 1.,
            aim: Vec3::new(1., 0., 0.),
            speed: 0.,
            move_target: None,
            selected: false,
        };
        let entity = world.push((
            transform_component,
            MeshComponent {
                render_node: mesh_render_node.clone(),
            },
            unit_component,
        ));
        let mut entry = world.entry(entity).unwrap();
        entry.add_component(VisibilityComponent {
            handle: {
                let handle = visibility_region.register_dynamic_object(
                    EntityId::from(entity),
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
                handle.add_feature(mesh_render_node.as_raw_generic_handle());
                handle
            },
        });
    }
}
