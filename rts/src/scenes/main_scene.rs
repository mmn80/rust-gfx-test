use super::{Scene, SceneManagerAction};
use crate::{
    camera::RTSCamera,
    dyn_object::DynObjectsState,
    input::{InputResource, KeyboardKey},
    kin_object::KinObjectsState,
    time::TimeState,
    RenderOptions,
};
use distill::loader::handle::Handle;
use glam::Vec3;
use legion::{IntoQuery, Resources, World, Write};
use rafx::{
    assets::distill_impl::AssetResource,
    renderer::ViewportsResource,
    visibility::{ViewFrustumArc, VisibilityRegion},
};
use rafx_plugins::{
    assets::font::FontAsset, components::DirectionalLightComponent, features::text::TextResource,
};

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

        let font = {
            let asset_resource = resources.get_mut::<AssetResource>().unwrap();
            asset_resource.load_asset_path::<FontAsset, _>("fonts/mplus-1p-regular.ttf")
        };

        let visibility_region = resources.get::<VisibilityRegion>().unwrap();
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
            let input = resources.get::<InputResource>().unwrap();
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
            let input = resources.get::<InputResource>().unwrap();
            if input.is_key_just_up(KeyboardKey::Escape) {
                SceneManagerAction::Scene(Scene::Menu)
            } else {
                SceneManagerAction::None
            }
        }
    }

    fn cleanup(&mut self, _world: &mut World, _resources: &Resources) {}
}
