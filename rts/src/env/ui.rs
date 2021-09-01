use egui::{Button, Checkbox, Ui};

use super::terrain::{Terrain, TerrainFillStyle};
use crate::{
    assets::env_tileset::LoadedEnvTileSet,
    env::perlin::PerlinNoise2D,
    ui::{SpawnMode, UiState},
};

pub enum EnvUiCmd {
    StartEditTile {
        tileset_name: String,
        tile_name: String,
    },
    SaveEditedTile(String),
    ResetTerrain(TerrainResetUiState),
}

pub struct TileSpawnUiState {
    pub active: bool,
    pub mode: SpawnMode,
    pub tileset: String,
    pub tile: String,
}

impl Default for TileSpawnUiState {
    fn default() -> Self {
        Self {
            active: false,
            mode: SpawnMode::OneShot,
            tileset: "Base".to_string(),
            tile: "Bilding".to_string(),
        }
    }
}

impl TileSpawnUiState {
    pub fn ui(ui_state: &mut UiState, ui: &mut Ui, tilesets: &Vec<LoadedEnvTileSet>) {
        let ed = &mut ui_state.env.tile_spawn;
        if ed.active {
            egui::CollapsingHeader::new("Spawn terrain object")
                .default_open(true)
                .show(ui, |ui| {
                    ed.mode.ui(ui, &mut ed.active);
                    ui.label("Click a location on the map to spawn terrain object");
                });
        } else if !ui_state.unit.spawning {
            egui::CollapsingHeader::new("Spawn terrain object")
                .default_open(true)
                .show(ui, |ui| {
                    ed.mode.ui(ui, &mut ed.active);
                    for tileset in tilesets {
                        ui.label(&tileset.name);
                        ui.horizontal_wrapped(|ui| {
                            for tile in &tileset.tiles {
                                if ui
                                    .selectable_label(false, format!("{}", &tile.inner.name))
                                    .clicked()
                                {
                                    ed.tile = tile.inner.name.clone();
                                    ed.active = true;
                                }
                            }
                        });
                    }
                });
        }
    }
}

pub struct TileEditUiState {
    pub active: bool,
    pub new_tile: bool,
    pub tileset: String,
    pub tile: String,
}

impl Default for TileEditUiState {
    fn default() -> Self {
        Self {
            active: false,
            new_tile: false,
            tileset: "".to_string(),
            tile: "".to_string(),
        }
    }
}

impl TileEditUiState {
    pub fn ui<F>(
        ui_state: &mut UiState,
        ui: &mut Ui,
        tilesets: &Vec<LoadedEnvTileSet>,
        mut cmd_exec: F,
    ) where
        F: FnMut(EnvUiCmd) -> Option<()>,
    {
        egui::CollapsingHeader::new("Edit terrain object")
            .default_open(false)
            .show(ui, |ui| {
                let ed = &mut ui_state.env.tile_edit;
                let mut editing_started = false;
                let mut editing_finished = false;
                let mut editing_failed = false;
                if ed.active {
                    let tileset = ed.tileset.clone();
                    let tile = ed.tile.clone();
                    if ed.new_tile {
                        ui.label(format!("Adding new tile to '{}':", tileset));
                        ui.text_edit_singleline(&mut ed.tile);
                    } else {
                        ui.label(format!(
                            "Editing tile '{}' from tileset '{}'",
                            tile, tileset
                        ));
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_sized([100., 30.], Button::new("Save")).clicked() {
                            editing_failed = tile.is_empty()
                                || cmd_exec(EnvUiCmd::SaveEditedTile(tile.clone())).is_none();
                            editing_finished = !editing_failed;
                        }
                        if ui.add_sized([100., 30.], Button::new("Quit")).clicked() {
                            editing_finished = true;
                        }
                    });
                } else {
                    for tileset in tilesets {
                        let tileset_name = tileset.name.clone();
                        ui.label(&tileset_name);
                        ui.horizontal_wrapped(|ui| {
                            for tile in &tileset.tiles {
                                if ui
                                    .selectable_label(false, format!("{}", &tile.inner.name))
                                    .clicked()
                                {
                                    ed.active = true;
                                    ed.new_tile = false;
                                    ed.tileset = tileset_name.clone();
                                    ed.tile = tile.inner.name.clone();
                                    editing_started = true;
                                    cmd_exec(EnvUiCmd::StartEditTile {
                                        tileset_name: tileset_name.clone(),
                                        tile_name: tile.inner.name.clone(),
                                    });
                                }
                            }
                            if ui.selectable_label(false, "+").clicked() {
                                ed.active = true;
                                ed.new_tile = true;
                                ed.tileset = tileset_name.clone();
                                ed.tile = "".to_string();
                                editing_started = true;
                                cmd_exec(EnvUiCmd::StartEditTile {
                                    tileset_name: tileset_name.clone(),
                                    tile_name: "".to_string(),
                                });
                            }
                        });
                    }
                };
                if editing_started {
                    ui_state.env.terrain_edit.active = true;
                }
                if editing_finished {
                    ed.active = false;
                    ed.new_tile = false;
                    ed.tileset = "".to_string();
                    ed.tile = "".to_string();
                    cmd_exec(EnvUiCmd::ResetTerrain(ui_state.env.terrain_reset.clone()));
                }
                if editing_failed {
                    ui_state.error(format!("Exporting tile failed."));
                }
            });
    }
}

pub struct TerrainEditUiState {
    pub active: bool,
    pub material: &'static str,
}

impl Default for TerrainEditUiState {
    fn default() -> Self {
        Self {
            active: false,
            material: "basic_tile",
        }
    }
}

impl TerrainEditUiState {
    pub fn ui(ui_state: &mut UiState, ui: &mut Ui) {
        let ed = &mut ui_state.env.terrain_edit;
        egui::CollapsingHeader::new("Edit terrain")
            .default_open(true)
            .show(ui, |ui| {
                let ck = Checkbox::new(&mut ed.active, "Edit mode active");
                ui.add(ck);
                if ed.active {
                    ui.label("Build material:");
                    for material_name in Terrain::get_default_material_names() {
                        ui.radio_value(&mut ed.material, material_name, material_name);
                    }
                }
            });
    }
}

#[derive(Clone)]
pub struct TerrainResetUiState {
    pub size: u32,
    pub style: TerrainFillStyle,
}

impl Default for TerrainResetUiState {
    fn default() -> Self {
        Self {
            size: 4096,
            style: TerrainFillStyle::FlatBoard {
                material: "basic_tile",
            },
        }
    }
}

impl TerrainResetUiState {
    pub fn ui<F>(ui_state: &mut UiState, ui: &mut Ui, mut cmd_exec: F)
    where
        F: FnMut(EnvUiCmd) -> Option<()>,
    {
        egui::CollapsingHeader::new("Reset terrain")
            .default_open(true)
            .show(ui, |ui| {
                let ed = &mut ui_state.env.terrain_reset;

                let mut size_str = format!("{}", ed.size);
                ui.horizontal(|ui| {
                    ui.label("Size");
                    ui.text_edit_singleline(&mut size_str);
                    if let Ok(number) = size_str.parse() {
                        ed.size = number;
                    }
                });
                let mut style_idx = match ed.style {
                    TerrainFillStyle::FlatBoard { material: _ } => 0,
                    TerrainFillStyle::CheckersBoard { zero: _, one: _ } => 1,
                    TerrainFillStyle::PerlinNoise {
                        params: _,
                        material: _,
                    } => 2,
                };
                ui.radio_value(&mut style_idx, 0, "Flat board");
                ui.radio_value(&mut style_idx, 1, "Checkers board");
                ui.radio_value(&mut style_idx, 2, "Perlin noise");

                ui.add_space(10.);

                let materials = Terrain::get_default_material_names();
                if style_idx == 0 {
                    let material = if let TerrainFillStyle::FlatBoard { material } = ed.style {
                        material
                    } else {
                        "basic_tile"
                    };
                    let material = UiState::combo_box(ui, &materials, material, "mat");
                    ed.style = TerrainFillStyle::FlatBoard { material };
                } else if style_idx == 1 {
                    let (zero, one) =
                        if let TerrainFillStyle::CheckersBoard { zero, one } = ed.style {
                            (zero, one)
                        } else {
                            ("basic_tile", "black_plastic")
                        };
                    let zero = UiState::combo_box(ui, &materials, zero, "zero");
                    let one = UiState::combo_box(ui, &materials, one, "one");
                    ed.style = TerrainFillStyle::CheckersBoard { zero, one };
                } else if style_idx == 2 {
                    let (mut params, material) =
                        if let TerrainFillStyle::PerlinNoise { params, material } = ed.style {
                            (params, material)
                        } else {
                            (
                                PerlinNoise2D {
                                    octaves: 6,
                                    amplitude: 10.0,
                                    frequency: 1.0,
                                    persistence: 1.0,
                                    lacunarity: 2.0,
                                    scale: (ed.size as f64, ed.size as f64),
                                    bias: 0.,
                                    seed: 42,
                                },
                                "basic_tile",
                            )
                        };
                    let material = UiState::combo_box(ui, &materials, material, "mat");
                    ui.add(egui::Slider::new(&mut params.octaves, 0..=8).text("octaves"));
                    ui.add(egui::Slider::new(&mut params.amplitude, 0.0..=64.0).text("amplitude"));
                    ui.add(egui::Slider::new(&mut params.frequency, 0.0..=4.0).text("frequency"));
                    ui.add(
                        egui::Slider::new(&mut params.persistence, 0.0..=2.0).text("persistence"),
                    );
                    ui.add(egui::Slider::new(&mut params.lacunarity, 1.0..=4.0).text("lacunarity"));
                    ui.add(
                        egui::Slider::new(&mut params.bias, 0.0..=ed.size as f64 + 1.).text("bias"),
                    );
                    ui.add(egui::Slider::new(&mut params.seed, 0..=16384).text("seed"));

                    ed.style = TerrainFillStyle::PerlinNoise { params, material };
                }
                ui.add_space(10.);
                if ui
                    .add_sized([100., 30.], Button::new("Reset terrain"))
                    .clicked()
                {
                    cmd_exec(EnvUiCmd::ResetTerrain(ui_state.env.terrain_reset.clone()));
                }
            });
    }
}

pub struct EnvUiState {
    pub tile_spawn: TileSpawnUiState,
    pub tile_edit: TileEditUiState,
    pub terrain_edit: TerrainEditUiState,
    pub terrain_reset: TerrainResetUiState,
}

impl Default for EnvUiState {
    fn default() -> Self {
        Self {
            tile_spawn: Default::default(),
            tile_edit: Default::default(),
            terrain_edit: Default::default(),
            terrain_reset: Default::default(),
        }
    }
}
