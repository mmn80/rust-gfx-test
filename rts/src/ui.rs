use egui::{Align, Checkbox, Color32};
use glam::Vec4;
use legion::{Resources, World};
use rafx::render_feature_renderer_prelude::AssetResource;
use rafx_plugins::features::egui::EguiContextResource;

use crate::{
    env::{env_object::EnvObjectsState, EnvUiState},
    scenes::MainState,
    time::TimeState,
    unit::unit::{UnitUiState, UnitsState},
    DebugUiState, RenderOptions,
};

#[derive(PartialEq, Eq, Clone)]
pub enum SpawnMode {
    OneShot,
    MultiShot,
}

impl SpawnMode {
    pub fn ui(&mut self, ui: &mut egui::Ui, spawning: &mut bool) {
        let mut multi_spawn = *self == SpawnMode::MultiShot;
        let ck = Checkbox::new(&mut multi_spawn, "Multi spawn mode");
        let changed = ui.add(ck).changed();
        if !multi_spawn && changed {
            *spawning = false;
        }
        *self = if multi_spawn {
            SpawnMode::MultiShot
        } else {
            SpawnMode::OneShot
        }
    }
}

pub struct UiState {
    pub main_light_rotates: bool,
    pub main_light_pitch: f32,
    pub main_light_color: Vec4,
    pub unit: UnitUiState,
    pub env: EnvUiState,
    error: String,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            main_light_rotates: true,
            main_light_pitch: 225.0,
            main_light_color: Vec4::ONE,
            unit: Default::default(),
            env: Default::default(),
            error: "".to_string(),
        }
    }
}

impl UiState {
    #[profiling::function]
    pub fn update(
        &mut self,
        world: &mut World,
        resources: &mut Resources,
        main_state: Option<&mut MainState>,
        kin_state: Option<&mut EnvObjectsState>,
        dyn_state: Option<&mut UnitsState>,
    ) {
        let context = resources.get::<EguiContextResource>().unwrap().context();
        profiling::scope!("egui");
        egui::SidePanel::left("ui_panel", 250.0).show(&context, |ui| {
            {
                let time_state = resources.get::<TimeState>().unwrap();
                let mut debug_ui_state = resources.get_mut::<DebugUiState>().unwrap();
                let mut render_options = resources.get_mut::<RenderOptions>().unwrap();
                let asset_manager = resources.get::<AssetResource>().unwrap();

                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(), |ui| {
                        ui.label(format!("Frame: {}", time_state.update_count()));
                        ui.separator();
                        ui.label(format!(
                            "FPS: {:.1}",
                            time_state.updates_per_second_smoothed()
                        ));
                    });
                });

                egui::CollapsingHeader::new("Options")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.checkbox(&mut debug_ui_state.show_render_options, "Render options");
                        ui.checkbox(&mut debug_ui_state.show_asset_list, "Asset list");

                        #[cfg(feature = "profile-with-puffin")]
                        if ui
                            .checkbox(&mut debug_ui_state.show_profiler, "Profiler")
                            .changed()
                        {
                            log::info!(
                                "Setting puffin profiler enabled: {:?}",
                                debug_ui_state.show_profiler
                            );
                            profiling::puffin::set_scopes_on(debug_ui_state.show_profiler);
                        }
                    });

                if debug_ui_state.show_render_options {
                    egui::CollapsingHeader::new("Render options")
                        .default_open(true)
                        .show(ui, |ui| {
                            render_options.ui(ui);
                        });
                }

                if debug_ui_state.show_asset_list {
                    egui::CollapsingHeader::new("Asset list")
                        .default_open(true)
                        .show(ui, |ui| {
                            egui::ScrollArea::from_max_height(400.).show(ui, |ui| {
                                let loader = asset_manager.loader();
                                let mut asset_info = loader
                                    .get_active_loads()
                                    .into_iter()
                                    .map(|item| loader.get_load_info(item))
                                    .collect::<Vec<_>>();
                                asset_info.sort_by(|x, y| {
                                    x.as_ref()
                                        .map(|x| &x.path)
                                        .cmp(&y.as_ref().map(|y| &y.path))
                                });
                                for info in asset_info {
                                    if let Some(info) = info {
                                        let id = info.asset_id;
                                        let _res = ui.selectable_label(
                                            false,
                                            format!(
                                                "{}:{} .. {}",
                                                info.file_name.unwrap_or_else(|| "???".to_string()),
                                                info.asset_name
                                                    .unwrap_or_else(|| format!("{}", id)),
                                                info.refs
                                            ),
                                        );
                                    } else {
                                        ui.label("NO INFO");
                                    }
                                }
                            });
                        });
                }

                #[cfg(feature = "profile-with-puffin")]
                if debug_ui_state.show_profiler {
                    profiling::scope!("puffin profiler");
                    puffin_egui::profiler_window(&context);
                }
            }

            if let Some(main_state) = main_state {
                main_state.update_ui(world, resources, self, ui);
            }
            if let Some(kin_state) = kin_state {
                kin_state.update_ui(world, resources, self, ui);
            }
            if let Some(dyn_state) = dyn_state {
                dyn_state.update_ui(world, resources, self, ui);
            }

            if !self.error.is_empty() {
                ui.with_layout(egui::Layout::bottom_up(Align::Center), |ui| {
                    ui.visuals_mut().override_text_color = Some(Color32::RED);
                    ui.style_mut().body_text_style = egui::TextStyle::Heading;
                    if ui.selectable_label(false, &self.error).clicked() {
                        self.error.clear();
                    }
                });
            }
        });
    }

    pub fn error(&mut self, message: String) {
        self.error = message.clone();
        log::error!("{}", message);
    }

    pub fn combo_box(
        ui: &mut egui::Ui,
        list: &Vec<&'static str>,
        current: &'static str,
        label: &'static str,
    ) -> &'static str {
        let mut result = current;
        egui::ComboBox::from_label(label)
            .selected_text(current)
            .width(150.0)
            .show_ui(ui, |ui| {
                for elem in list {
                    ui.selectable_value(&mut result, elem, elem);
                }
            });
        result
    }
}
