use super::{Scene, SceneManagerAction};
use crate::assets::font::FontAsset;
use crate::assets::gltf::MeshAsset;
use crate::camera::RTSCamera;
use crate::components::{
    DirectionalLightComponent, MeshComponent, PointLightComponent, TransformComponent,
};
use crate::components::{SpotLightComponent, VisibilityComponent};
use crate::features::mesh::{MeshRenderNode, MeshRenderNodeSet};
use crate::features::text::TextResource;
use crate::time::TimeState;
use crate::RenderOptions;
use distill::loader::handle::Handle;
use glam::Vec3;
use legion::IntoQuery;
use legion::{Read, Resources, World, Write};
use rafx::assets::distill_impl::AssetResource;
use rafx::assets::AssetManager;
use rafx::renderer::ViewportsResource;
use rafx::visibility::{CullModel, EntityId, ViewFrustumArc, VisibilityRegion};
use rand::{thread_rng, Rng};
use sdl2::{event::Event, keyboard::Keycode};

pub(super) struct ShadowsScene {
    main_view_frustum: ViewFrustumArc,
    font: Handle<FontAsset>,
    text_size: f32,
}

impl ShadowsScene {
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

        let mut load_visible_bounds = |asset_handle: &Handle<MeshAsset>| {
            asset_manager
                .wait_for_asset_to_load(asset_handle, &mut asset_resource, "")
                .unwrap();

            CullModel::VisibleBounds(
                asset_manager
                    .committed_asset(&floor_mesh_asset)
                    .unwrap()
                    .inner
                    .asset_data
                    .visible_bounds,
            )
        };

        //
        // Add a floor
        //
        {
            let position = Vec3::new(0.0, 0.0, -1.0);

            let floor_mesh = mesh_render_nodes.register_mesh(MeshRenderNode {
                mesh: floor_mesh_asset.clone(),
            });

            let transform_component = TransformComponent {
                translation: position,
                ..Default::default()
            };

            let mesh_component = MeshComponent {
                render_node: floor_mesh.clone(),
            };

            let entity = world.push((transform_component.clone(), mesh_component));
            let mut entry = world.entry(entity).unwrap();
            entry.add_component(VisibilityComponent {
                handle: {
                    let handle = visibility_region.register_static_object(
                        EntityId::from(entity),
                        load_visible_bounds(&floor_mesh_asset),
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

        //
        // Add some meshes
        //
        {
            let example_meshes = {
                let mut meshes = Vec::default();

                // container1
                meshes.push(mesh_render_nodes.register_mesh(MeshRenderNode {
                    mesh: container_1_asset,
                }));

                // container2
                meshes.push(mesh_render_nodes.register_mesh(MeshRenderNode {
                    mesh: container_2_asset,
                }));

                // blue icosphere - load by UUID since it's one of several meshes in the file
                meshes.push(mesh_render_nodes.register_mesh(MeshRenderNode {
                    mesh: blue_icosphere_asset,
                }));

                meshes
            };

            let mut rng = thread_rng();
            for i in 0..100 {
                let position = Vec3::new(((i / 9) * 3) as f32, ((i % 9) * 3) as f32, 0.0);
                let mesh_render_node = example_meshes[i % example_meshes.len()].clone();
                let asset_handle = &mesh_render_nodes.get(&mesh_render_node).unwrap().mesh;

                let rand_scale_z = rng.gen_range(0.8, 1.2);
                let offset = rand_scale_z - 1.;
                let transform_component = TransformComponent {
                    translation: position + Vec3::new(0., 0., offset),
                    scale: Vec3::new(
                        rng.gen_range(0.8, 1.2),
                        rng.gen_range(0.8, 1.2),
                        rand_scale_z,
                    ),
                    ..Default::default()
                };

                let mesh_component = MeshComponent {
                    render_node: mesh_render_node.clone(),
                };

                let entity = world.push((transform_component.clone(), mesh_component));
                let mut entry = world.entry(entity).unwrap();
                entry.add_component(VisibilityComponent {
                    handle: {
                        let handle = visibility_region.register_dynamic_object(
                            EntityId::from(entity),
                            load_visible_bounds(&asset_handle),
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
            //glam::Vec3::new(-3.0, 3.0, 2.0),
            glam::Vec3::new(5.0, 5.0, 2.0),
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
        let light_from = glam::Vec3::new(-5.0, 5.0, 5.0);
        let light_to = glam::Vec3::ZERO;
        let light_direction = (light_to - light_from).normalize();
        super::add_directional_light(
            resources,
            world,
            DirectionalLightComponent {
                direction: light_direction,
                intensity: 1.0,
                color: [0.0, 0.0, 1.0, 1.0].into(),
                view_frustum: visibility_region.register_view_frustum(),
            },
        );

        //
        // SPOT LIGHT
        //
        let light_from = glam::Vec3::new(-3.0, -3.0, 5.0);
        let light_to = glam::Vec3::ZERO;
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

        let text_size = 15.;

        ShadowsScene {
            main_view_frustum,
            font,
            text_size,
        }
    }
}

impl super::GameScene for ShadowsScene {
    fn update(
        &mut self,
        world: &mut World,
        resources: &mut Resources,
        events: Vec<Event>,
    ) -> SceneManagerAction {
        super::add_light_debug_draw(&resources, &world);

        {
            let time_state = resources.get::<TimeState>().unwrap();
            let mut viewports_resource = resources.get_mut::<ViewportsResource>().unwrap();
            let render_options = resources.get::<RenderOptions>().unwrap();
            let mut camera = resources.get_mut::<RTSCamera>().unwrap();

            camera.update_main_view_3d(
                &*time_state,
                &*render_options,
                &mut self.main_view_frustum,
                &mut *viewports_resource,
                &events,
            );
        }

        {
            let time_state = resources.get::<TimeState>().unwrap();
            let mut query = <Write<DirectionalLightComponent>>::query();
            for mut light in query.iter_mut(world) {
                const LIGHT_XY_DISTANCE: f32 = 50.0;
                const LIGHT_Z: f32 = 50.0;
                const LIGHT_ROTATE_SPEED: f32 = 0.0;
                const LIGHT_LOOP_OFFSET: f32 = 2.0;
                let loop_time = time_state.total_time().as_secs_f32();
                let light_from = glam::Vec3::new(
                    LIGHT_XY_DISTANCE
                        * f32::cos(LIGHT_ROTATE_SPEED * loop_time + LIGHT_LOOP_OFFSET),
                    LIGHT_XY_DISTANCE
                        * f32::sin(LIGHT_ROTATE_SPEED * loop_time + LIGHT_LOOP_OFFSET),
                    LIGHT_Z,
                    //LIGHT_Z// * f32::sin(LIGHT_ROTATE_SPEED * loop_time + LIGHT_LOOP_OFFSET).abs(),
                    //0.2
                    //2.0
                );
                let light_to = glam::Vec3::default();

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
                let light_from = glam::Vec3::new(
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
            let mut text_resource = resources.get_mut::<TextResource>().unwrap();

            text_resource.add_text(
                "Lorem Ipsum".to_string(),
                glam::Vec3::new(100.0, 400.0, 0.0),
                &self.font,
                20.0,
                glam::Vec4::new(1.0, 0.0, 0.0, 1.0),
            );
            text_resource.add_text(
                "Lorem Ipsum".to_string(),
                glam::Vec3::new(100.0, 430.0, 0.0),
                &self.font,
                25.0,
                glam::Vec4::new(0.0, 1.0, 0.0, 1.0),
            );
            text_resource.add_text(
                "Lorem Ipsum".to_string(),
                glam::Vec3::new(100.0, 460.0, 0.0),
                &self.font,
                30.0,
                glam::Vec4::new(0.0, 0.0, 1.0, 1.0),
            );
            text_resource.add_text(
                "Lorem Ipsum".to_string(),
                glam::Vec3::new(100.0, 500.0, 0.0),
                &self.font,
                35.0,
                glam::Vec4::new(1.0, 1.0, 1.0, 1.0),
            );
            let font_size = self.text_size.min(100.).max(5.).round();
            text_resource.add_text(format!("Font size: {}px.
Veritatis incidunt tempore eum voluptas. At excepturi corporis ullam. Ab sint omnis illum possimus.
Quis voluptatum et et quibusdam. Inventore eaque id atque veritatis dolor autem veritatis.

Maxime non cum tempore. Quia est modi voluptatem omnis totam culpa.
Qui voluptatem molestias repudiandae veritatis nostrum.
Reiciendis facere et eum sit quis.

Facere qui debitis eligendi dolores laboriosam. Qui ut quis voluptatem excepturi natus accusamus.
Velit consequuntur quis sunt unde distinctio quae.
Quas mollitia vel dicta impedit earum nesciunt sapiente libero. Est consequatur odit dolor rerum.

Ut voluptatem autem eos. Veniam voluptatem voluptatem fuga dolorem voluptatibus ducimus veniam alias.
Atque at itaque minima enim dolorem vero libero officia. Itaque voluptatibus rerum non sapiente assumenda libero sint non.
Autem quibusdam nam officiis quia et ducimus qui. Est sed excepturi et ab ut sit quia provident.

Quis deserunt enim eligendi sed. Ab adipisci minus quo tenetur nihil debitis sapiente distinctio.
Dolores repudiandae minus qui est itaque. Aspernatur fuga qui consequatur placeat nisi adipisci nostrum.", font_size),
                glam::Vec3::new(400.0, 400.0, 0.0),
                &self.font,
                font_size,
                glam::Vec4::new(1.0, 1.0, 1.0, 1.0),
            );
        }

        let mut action = SceneManagerAction::None;
        for event in events {
            match event {
                Event::KeyDown {
                    keycode: Some(keycode),
                    keymod: _modifiers,
                    ..
                } => {
                    if keycode == Keycode::Escape {
                        action = SceneManagerAction::Scene(Scene::Menu);
                    }
                    if keycode == Keycode::Equals {
                        let time = resources.get::<TimeState>().unwrap();
                        self.text_size = (self.text_size + 40. * time.previous_update_dt())
                            .min(100.)
                            .max(5.);
                    }
                    if keycode == Keycode::Minus {
                        let time = resources.get::<TimeState>().unwrap();
                        self.text_size = (self.text_size - 40. * time.previous_update_dt())
                            .min(100.)
                            .max(5.);
                    }
                }
                _ => {}
            }
        }
        action
    }
}
