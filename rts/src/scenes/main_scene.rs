use distill::loader::handle::Handle;
use glam::{Vec3, Vec4};
use legion::Resources;
use rafx::{
    assets::{distill_impl::AssetResource, AssetManager},
    renderer::ViewportsResource,
};
use rafx_plugins::{
    assets::font::FontAsset,
    features::{mesh_basic::MeshBasicRenderOptions, text::TextResource},
};

use super::{Scene, SceneManagerAction};
use crate::{
    camera::RTSCamera,
    env::{env::EnvState, simulation::Simulation},
    input::{InputResource, KeyboardKey},
    ui::UiState,
    unit::unit::UnitsState,
    RenderOptions,
};

pub struct MainState {}

impl MainState {
    pub fn update_ui(
        &mut self,
        _simulation: &mut Simulation,
        resources: &mut Resources,
        ui_state: &mut UiState,
        ui: &mut egui::Ui,
    ) {
        {
            let mut camera = resources.get_mut::<RTSCamera>().unwrap();
            camera.update_ui(ui_state, ui);
        }

        egui::CollapsingHeader::new("Directional light")
            .default_open(false)
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
    font: Handle<FontAsset>,
    main_state: MainState,
    units: UnitsState,
    env: EnvState,
}

impl MainScene {
    pub(super) fn new(simulation: &mut Simulation, resources: &Resources) -> Self {
        let mut render_options = resources.get_mut::<RenderOptions>().unwrap();
        *render_options = RenderOptions::default_3d();

        let mut mesh_render_options = resources.get_mut::<MeshBasicRenderOptions>().unwrap();
        mesh_render_options.ambient_light = glam::Vec3::new(0.005, 0.005, 0.005);

        let font = {
            let asset_resource = resources.get_mut::<AssetResource>().unwrap();
            asset_resource.load_asset_path::<FontAsset, _>("fonts/mplus-1p-regular.ttf")
        };

        let env = EnvState::new(resources, simulation);
        let units = UnitsState::new(resources);

        MainScene {
            font,
            main_state: MainState {},
            units,
            env,
        }
    }
}

impl super::GameScene for MainScene {
    fn update(
        &mut self,
        simulation: &mut Simulation,
        resources: &mut Resources,
        ui_state: &mut UiState,
    ) -> SceneManagerAction {
        //super::add_light_debug_draw(&resources, &world);

        ui_state.update(
            simulation,
            resources,
            Some(&mut self.main_state),
            Some(&mut self.env),
            Some(&mut self.units),
        );

        self.env.update(simulation, resources, ui_state);
        self.units.update(simulation, resources, ui_state);

        {
            let asset_manager = resources.get::<AssetManager>().unwrap();
            if asset_manager.committed_asset(&self.font).is_some() {
                let viewports_resource = resources.get::<ViewportsResource>().unwrap();
                let mut text_resource = resources.get_mut::<TextResource>().unwrap();
                let camera = resources.get::<RTSCamera>().unwrap();
                let scale = camera.win_scale_factor;
                let pos_y = viewports_resource.main_window_size.height as f32 - 30. * scale;
                text_resource.add_text(
                    format!("camera: {:.2}m", camera.look_at_dist),
                    Vec3::new(300.0 * scale, pos_y, 0.0),
                    &self.font,
                    20.0 * scale,
                    glam::Vec4::new(1.0, 1.0, 1.0, 1.0),
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

    fn cleanup(&mut self, simulation: &mut Simulation, _resources: &Resources) {
        simulation.reset();
    }
}
