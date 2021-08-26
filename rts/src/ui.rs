use legion::{Resources, World};
use rafx_plugins::features::egui::EguiContextResource;

use crate::{
    dyn_object::{DynObjectType, DynObjectsState},
    kin_object::{KinObjectType, KinObjectsState},
    terrain::TerrainFillStyle,
};

pub struct UiState {
    pub dyn_spawning: bool,
    pub dyn_object_type: DynObjectType,
    pub dyn_selecting: bool,
    pub dyn_selected_count: u32,
    pub dyn_selected_str: String,
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
            dyn_selected_str: "".to_string(),
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
        kin_state: &mut KinObjectsState,
        dyn_state: &mut DynObjectsState,
    ) {
        let context = resources.get::<EguiContextResource>().unwrap().context();
        profiling::scope!("egui");
        egui::SidePanel::left("ui_panel", 250.0).show(&context, |ui| {
            kin_state.update_ui(world, resources, self, ui);
            dyn_state.update_ui(world, resources, self, ui);
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
