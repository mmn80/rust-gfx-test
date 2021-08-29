use distill::loader::handle::Handle;
use glam::{Quat, Vec3, Vec4};
use legion::{Entity, Resources, World};
use rafx::{
    assets::distill_impl::AssetResource,
    renderer::ViewportsResource,
    visibility::{ViewFrustumArc, VisibilityRegion},
};
use rafx_plugins::{
    assets::font::FontAsset, components::DirectionalLightComponent, features::text::TextResource,
};

use super::{Scene, SceneManagerAction};
use crate::{
    camera::RTSCamera,
    env::env_object::EnvObjectsState,
    input::{InputResource, KeyboardKey},
    time::TimeState,
    ui::UiState,
    unit::unit::UnitsState,
    RenderOptions,
};

pub struct MainState {}

impl MainState {
    pub fn update_ui(
        &mut self,
        _world: &mut World,
        _resources: &mut Resources,
        ui_state: &mut UiState,
        ui: &mut egui::Ui,
    ) {
        egui::CollapsingHeader::new("Directional light")
            .default_open(true)
            .show(ui, |ui| {
                let ck = egui::Checkbox::new(&mut ui_state.main_light_rotates, "Auto rotates");
                ui.add(ck);
                if !ui_state.main_light_rotates {
                    ui.add(
                        egui::Slider::new(&mut ui_state.main_light_pitch, 180.0..=360.)
                            .text("pitch"),
                    );
                }
                ui.horizontal(|ui| {
                    ui.label("Color (rgb):");
                    let mut r_str = format!("{}", (ui_state.main_light_color.x * 256.) as u8);
                    ui.add(egui::TextEdit::singleline(&mut r_str).desired_width(30.));
                    let mut g_str = format!("{}", (ui_state.main_light_color.y * 256.) as u8);
                    ui.add(egui::TextEdit::singleline(&mut g_str).desired_width(30.));
                    let mut b_str = format!("{}", (ui_state.main_light_color.z * 256.) as u8);
                    ui.add(egui::TextEdit::singleline(&mut b_str).desired_width(30.));
                    if let (Ok(r), Ok(g), Ok(b)) = (
                        r_str.parse::<u8>(),
                        g_str.parse::<u8>(),
                        b_str.parse::<u8>(),
                    ) {
                        ui_state.main_light_color =
                            Vec4::new(r as f32 / 256., g as f32 / 256., b as f32 / 256., 1.);
                    }
                });
            });
    }
}

pub struct MainScene {
    main_view_frustum: ViewFrustumArc,
    font: Handle<FontAsset>,
    main_state: MainState,
    main_light: Entity,
    dyn_objects: UnitsState,
    kin_objects: EnvObjectsState,
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
        let light_from = Vec3::new(0.0, 5.0, 4.0);
        let light_to = Vec3::ZERO;
        let light_direction = (light_to - light_from).normalize();
        let main_light = world.push((DirectionalLightComponent {
            direction: light_direction,
            intensity: 5.0,
            color: [1.0, 1.0, 1.0, 1.0].into(),
            view_frustum: visibility_region.register_view_frustum(),
        },));

        let main_view_frustum = visibility_region.register_view_frustum();
        let kin_objects = EnvObjectsState::new(resources, world);
        let dyn_objects = UnitsState::new(resources, kin_objects.terrain.clone());

        MainScene {
            main_view_frustum,
            font,
            main_state: MainState {},
            main_light,
            dyn_objects,
            kin_objects,
        }
    }
}

impl super::GameScene for MainScene {
    fn update(
        &mut self,
        world: &mut World,
        resources: &mut Resources,
        ui_state: &mut UiState,
    ) -> SceneManagerAction {
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
            if let Some(mut entry) = world.entry(self.main_light) {
                if let Ok(light) = entry.get_component_mut::<DirectionalLightComponent>() {
                    if ui_state.main_light_rotates {
                        let time_state = resources.get::<TimeState>().unwrap();
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
                    } else {
                        let q = Quat::from_rotation_x(
                            ui_state.main_light_pitch * std::f32::consts::PI / 180.,
                        );
                        light.direction = q.mul_vec3(Vec3::Y);
                    }
                    light.color = ui_state.main_light_color;
                }
            }
        }

        ui_state.update(
            world,
            resources,
            Some(&mut self.main_state),
            Some(&mut self.kin_objects),
            Some(&mut self.dyn_objects),
        );

        self.kin_objects.update(world, resources);
        self.dyn_objects.update(world, resources, ui_state);

        {
            let viewports_resource = resources.get::<ViewportsResource>().unwrap();
            let mut text_resource = resources.get_mut::<TextResource>().unwrap();
            let camera = resources.get::<RTSCamera>().unwrap();
            let scale = camera.win_scale_factor;
            let pos_y = viewports_resource.main_window_size.height as f32 - 30. * scale;
            text_resource.add_text(
                format!("camera: {:.2}m", camera.look_at_dist),
                Vec3::new(300.0, pos_y, 0.0),
                &self.font,
                20.0 * scale,
                glam::Vec4::new(1.0, 1.0, 1.0, 1.0),
            );
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
