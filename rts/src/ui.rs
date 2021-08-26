use std::collections::HashMap;

use legion::{Resources, World};
use rafx::render_feature_renderer_prelude::AssetResource;
use rafx_plugins::features::egui::EguiContextResource;

use crate::{
    dyn_object::{DynObjectType, DynObjectsState},
    kin_object::{KinObjectType, KinObjectsState},
    terrain::TerrainFillStyle,
    time::TimeState,
    DebugUiState, RenderOptions,
};

pub struct UiState {
    pub dyn_spawning: bool,
    pub dyn_object_type: DynObjectType,
    pub dyn_selecting: bool,
    pub dyn_selected_count: u32,
    pub dyn_selected: HashMap<DynObjectType, u32>,
    pub kin_spawning: bool,
    pub kin_object_type: KinObjectType,
    pub kin_edit_mode: bool,
    pub kin_edit_material: &'static str,
    pub kin_terrain_size: u32,
    pub kin_terrain_style: TerrainFillStyle,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            dyn_spawning: false,
            dyn_object_type: DynObjectType::Container1,
            dyn_selecting: false,
            dyn_selected_count: 0,
            dyn_selected: Default::default(),
            kin_spawning: false,
            kin_object_type: KinObjectType::Building,
            kin_edit_mode: false,
            kin_edit_material: "simple_tile",
            kin_terrain_size: 4096,
            kin_terrain_style: TerrainFillStyle::FlatBoard {
                material: "simple_tile",
            },
        }
    }
}

impl UiState {
    pub fn update(
        &mut self,
        world: &mut World,
        resources: &mut Resources,
        kin_state: Option<&mut KinObjectsState>,
        dyn_state: Option<&mut DynObjectsState>,
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
            if let Some(kin_state) = kin_state {
                kin_state.update_ui(world, resources, self, ui);
            }
            if let Some(dyn_state) = dyn_state {
                dyn_state.update_ui(world, resources, self, ui);
            }
        });
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
