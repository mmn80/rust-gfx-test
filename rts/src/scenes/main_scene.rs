use super::{Scene, SceneManagerAction};
use crate::{
    assets::{font::FontAsset, gltf::MeshAsset},
    camera::RTSCamera,
    components::{
        DirectionalLightComponent, MeshComponent, TransformComponent, VisibilityComponent,
    },
    dyn_object::DynObjectsState,
    features::{
        mesh::{MeshRenderObject, MeshRenderObjectSet},
        text::TextResource,
    },
    input::InputState,
    kin_object::KinObjectsState,
    time::TimeState,
    RenderOptions,
};
use distill::loader::handle::Handle;
use glam::Vec3;
use legion::{IntoQuery, Resources, World, Write};
use rafx::{
    assets::{distill_impl::AssetResource, AssetManager},
    renderer::ViewportsResource,
    visibility::{CullModel, ObjectId, ViewFrustumArc, VisibilityRegion},
};
use winit::event::VirtualKeyCode;

pub(super) struct MainScene {
    main_view_frustum: ViewFrustumArc,
    font: Handle<FontAsset>,
    dyn_objects: DynObjectsState,
    kin_objects: KinObjectsState,
}

impl MainScene {
    pub(super) fn new(world: &mut World, resources: &Resources) -> Self {
        let mut render_options = resources.get_mut::<RenderOptions>().unwrap();
        *render_options = RenderOptions::default_3d();

        let visibility_region = resources.get::<VisibilityRegion>().unwrap();

        //
        // Add a floor
        //
        let font = {
            let mut asset_manager = resources.get_mut::<AssetManager>().unwrap();
            let mut asset_resource = resources.get_mut::<AssetResource>().unwrap();
            let floor_mesh_asset =
                asset_resource.load_asset_path::<MeshAsset, _>("blender/cement_floor.glb");
            asset_manager
                .wait_for_asset_to_load(&floor_mesh_asset, &mut asset_resource, "")
                .unwrap();

            const FLOOR_SIZE: f32 = 48.;
            const FLOOR_NUM: i32 = 10;

            let mut mesh_render_objects = resources.get_mut::<MeshRenderObjectSet>().unwrap();
            let floor_mesh = mesh_render_objects.register_render_object(MeshRenderObject {
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
                        render_object_handle: floor_mesh.clone(),
                    };

                    let entity = world.push((transform_component, mesh_component));
                    let mut entry = world.entry(entity).unwrap();
                    entry.add_component(VisibilityComponent {
                        visibility_object_handle: {
                            let handle = visibility_region.register_static_object(
                                ObjectId::from(entity),
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
                            handle.add_render_object(&floor_mesh);
                            handle
                        },
                    });
                }
            }
            asset_resource.load_asset_path::<FontAsset, _>("fonts/mplus-1p-regular.ttf")
        };

        //
        // Directional light
        //
        {
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
        }

        let main_view_frustum = visibility_region.register_view_frustum();
        let dyn_objects = DynObjectsState::new(resources);
        let kin_objects = KinObjectsState::new(resources);

        MainScene {
            main_view_frustum,
            font,
            dyn_objects,
            kin_objects,
        }
    }
}

impl super::GameScene for MainScene {
    fn update(&mut self, world: &mut World, resources: &mut Resources) -> SceneManagerAction {
        //super::add_light_debug_draw(&resources, &world);

        {
            let input = resources.get::<InputState>().unwrap();
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

        self.dyn_objects.update(world, resources);
        self.kin_objects.update(world, resources);

        {
            let viewports_resource = resources.get::<ViewportsResource>().unwrap();
            let mut text_resource = resources.get_mut::<TextResource>().unwrap();
            let camera = resources.get::<RTSCamera>().unwrap();
            let scale = camera.win_scale_factor;
            let pos_y = viewports_resource.main_window_size.height as f32 - 30. * scale;
            text_resource.add_text(
                format!("camera: {:.2}m", camera.look_at_dist),
                Vec3::new(10.0, pos_y, 0.0),
                &self.font,
                20.0 * scale,
                glam::Vec4::new(1.0, 1.0, 1.0, 1.0),
            );
            if self.dyn_objects.ui_selected_count > 0 {
                text_resource.add_text(
                    self.dyn_objects.ui_selected_str.clone(),
                    Vec3::new(200.0 * scale, pos_y, 0.0),
                    &self.font,
                    20.0 * scale,
                    glam::Vec4::new(0.5, 1.0, 0.5, 1.0),
                );
            }
        }

        {
            let input = resources.get::<InputState>().unwrap();
            if input.key_trigger.contains(&VirtualKeyCode::Escape) {
                SceneManagerAction::Scene(Scene::Menu)
            } else {
                SceneManagerAction::None
            }
        }
    }

    fn cleanup(&mut self, _world: &mut World, _resources: &Resources) {}
}
