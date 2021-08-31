use std::collections::HashMap;

use building_blocks::core::prelude::*;
use distill::loader::handle::Handle;
use egui::{Button, Checkbox};
use glam::{Quat, Vec3};
use legion::{Resources, World};
use rafx::assets::{distill_impl::AssetResource, AssetManager};
use rafx_plugins::components::TransformComponent;

use crate::{
    assets::{env_tile::EnvTileAsset, pbr_material::PbrMaterialAsset},
    camera::RTSCamera,
    env::{
        perlin::PerlinNoise2D,
        terrain::{Terrain, TerrainFillStyle, TerrainHandle, TerrainResource},
    },
    input::{InputResource, KeyboardKey, MouseButton},
    ui::{SpawnMode, UiState},
};

#[derive(Clone)]
pub struct EnvTileComponent {
    pub asset: Handle<EnvTileAsset>,
    pub health: f32,
    pub selected: bool,
}

pub struct EnvObjectsState {
    pub terrain: TerrainHandle,
    tiles: HashMap<String, Handle<EnvTileAsset>>,
}

impl EnvObjectsState {
    pub fn new(resources: &Resources, world: &mut World) -> Self {
        let asset_resource = resources.get::<AssetResource>().unwrap();

        log::info!("Loading terrain materials...");

        let material_names = Terrain::get_default_material_names();
        let terrain_materials: Vec<_> = material_names
            .iter()
            .map(|name| {
                let path = format!("materials/terrain/{}.pbrmaterial", *name);
                let material_handle = asset_resource.load_asset_path::<PbrMaterialAsset, _>(path);
                (*name, material_handle.clone())
            })
            .collect();

        log::info!("Terrain materials loaded");

        log::info!("Loading terrain tiles...");

        let tile_names = vec!["building", "tree"];
        let tiles: HashMap<String, Handle<EnvTileAsset>> = tile_names
            .iter()
            .map(|name| {
                let path = format!("tiles/{}.tile", *name);
                let tile_handle = asset_resource.load_asset_path::<EnvTileAsset, _>(path);
                (String::from(*name), tile_handle.clone())
            })
            .collect();

        log::info!("Terrain tiles loaded");

        let ui_terrain_size: u32 = 4096;
        let ui_terrain_style = TerrainFillStyle::FlatBoard {
            material: "basic_tile",
        };
        let terrain = {
            let mut terrain_resource = resources.get_mut::<TerrainResource>().unwrap();
            terrain_resource.new_terrain(
                world,
                terrain_materials,
                Point3i::ZERO,
                ui_terrain_size,
                ui_terrain_style.clone(),
            )
        };

        EnvObjectsState { terrain, tiles }
    }

    pub fn update_ui(
        &mut self,
        world: &mut World,
        resources: &mut Resources,
        ui_state: &mut UiState,
        ui: &mut egui::Ui,
    ) {
        let input = resources.get::<InputResource>().unwrap();
        let camera = resources.get::<RTSCamera>().unwrap();

        if ui_state.env.spawning {
            egui::CollapsingHeader::new("Spawn terrain object")
                .default_open(true)
                .show(ui, |ui| {
                    ui_state.env.spawn_mode.ui(ui, &mut ui_state.env.spawning);
                    ui.label("Click a location on the map to spawn terrain object");
                });
        } else if !ui_state.unit.spawning {
            egui::CollapsingHeader::new("Spawn terrain object")
                .default_open(true)
                .show(ui, |ui| {
                    ui_state.env.spawn_mode.ui(ui, &mut ui_state.env.spawning);
                    ui.horizontal_wrapped(|ui| {
                        for (name, _) in &self.tiles {
                            if ui.selectable_label(false, format!("{}", name)).clicked() {
                                ui_state.env.object_type = String::from(name);
                                ui_state.env.spawning = true;
                            }
                        }
                    });
                });

            egui::CollapsingHeader::new("Edit terrain")
                .default_open(true)
                .show(ui, |ui| {
                    let ck = Checkbox::new(&mut ui_state.env.edit_mode, "Edit mode active");
                    ui.add(ck);
                    if ui_state.env.edit_mode {
                        ui.label("Build material:");
                        for material_name in Terrain::get_default_material_names() {
                            ui.radio_value(
                                &mut ui_state.env.edit_material,
                                material_name,
                                material_name,
                            );
                        }
                    }
                });

            egui::CollapsingHeader::new("Reset terrain")
                .default_open(true)
                .show(ui, |ui| {
                    let mut size_str = format!("{}", ui_state.env.terrain_size);
                    ui.horizontal(|ui| {
                        ui.label("Size");
                        ui.text_edit_singleline(&mut size_str);
                        if let Ok(number) = size_str.parse() {
                            ui_state.env.terrain_size = number;
                        }
                    });
                    let mut style_idx = match ui_state.env.terrain_style {
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
                        let material = if let TerrainFillStyle::FlatBoard { material } =
                            ui_state.env.terrain_style
                        {
                            material
                        } else {
                            "basic_tile"
                        };
                        let material = UiState::combo_box(ui, &materials, material, "mat");
                        ui_state.env.terrain_style = TerrainFillStyle::FlatBoard { material };
                    } else if style_idx == 1 {
                        let (zero, one) = if let TerrainFillStyle::CheckersBoard { zero, one } =
                            ui_state.env.terrain_style
                        {
                            (zero, one)
                        } else {
                            ("basic_tile", "black_plastic")
                        };
                        let zero = UiState::combo_box(ui, &materials, zero, "zero");
                        let one = UiState::combo_box(ui, &materials, one, "one");
                        ui_state.env.terrain_style = TerrainFillStyle::CheckersBoard { zero, one };
                    } else if style_idx == 2 {
                        let (mut params, material) =
                            if let TerrainFillStyle::PerlinNoise { params, material } =
                                ui_state.env.terrain_style
                            {
                                (params, material)
                            } else {
                                (
                                    PerlinNoise2D {
                                        octaves: 6,
                                        amplitude: 10.0,
                                        frequency: 1.0,
                                        persistence: 1.0,
                                        lacunarity: 2.0,
                                        scale: (
                                            ui_state.env.terrain_size as f64,
                                            ui_state.env.terrain_size as f64,
                                        ),
                                        bias: 0.,
                                        seed: 42,
                                    },
                                    "basic_tile",
                                )
                            };
                        let material = UiState::combo_box(ui, &materials, material, "mat");
                        ui.add(egui::Slider::new(&mut params.octaves, 0..=8).text("octaves"));
                        ui.add(
                            egui::Slider::new(&mut params.amplitude, 0.0..=64.0).text("amplitude"),
                        );
                        ui.add(
                            egui::Slider::new(&mut params.frequency, 0.0..=4.0).text("frequency"),
                        );
                        ui.add(
                            egui::Slider::new(&mut params.persistence, 0.0..=2.0)
                                .text("persistence"),
                        );
                        ui.add(
                            egui::Slider::new(&mut params.lacunarity, 1.0..=4.0).text("lacunarity"),
                        );
                        ui.add(
                            egui::Slider::new(
                                &mut params.bias,
                                0.0..=ui_state.env.terrain_size as f64 + 1.,
                            )
                            .text("bias"),
                        );
                        ui.add(egui::Slider::new(&mut params.seed, 0..=16384).text("seed"));

                        ui_state.env.terrain_style =
                            TerrainFillStyle::PerlinNoise { params, material };
                    }
                    ui.add_space(10.);
                    if ui
                        .add_sized([100., 30.], Button::new("Reset terrain"))
                        .clicked()
                    {
                        let mut terrain_resource = resources.get_mut::<TerrainResource>().unwrap();
                        let mut storage = terrain_resource.write();
                        let terrain = storage.get_mut(&self.terrain);
                        terrain.reset(
                            world,
                            Point3i::ZERO,
                            ui_state.env.terrain_size,
                            ui_state.env.terrain_style.clone(),
                        );
                    }
                });
        }

        if ui_state.env.spawning || (ui_state.env.edit_mode && !ui_state.unit.spawning) {
            if input.is_mouse_button_just_clicked(MouseButton::LEFT) {
                let cursor_pos = input.mouse_position();
                let (cast_result, default_material) = {
                    let terrain_resource = resources.get::<TerrainResource>().unwrap();
                    let storage = terrain_resource.read();
                    let terrain = storage.get(&self.terrain);
                    let cast_result = camera.ray_cast_terrain(
                        cursor_pos.x as u32,
                        cursor_pos.y as u32,
                        terrain,
                        ui_state,
                    );
                    let default_material = terrain
                        .voxel_by_material(ui_state.env.edit_material)
                        .unwrap();
                    (cast_result, default_material)
                };
                if let Some(result) = cast_result {
                    if ui_state.env.spawning {
                        self.spawn(
                            ui_state.env.object_type.clone(),
                            PointN([result.hit.x(), result.hit.y(), result.hit.z() + 1]),
                            resources,
                            world,
                        );
                    } else if ui_state.env.edit_mode {
                        let mut terrain_resource = resources.get_mut::<TerrainResource>().unwrap();
                        let mut storage = terrain_resource.write();
                        let terrain = storage.get_mut(&self.terrain);
                        if input.is_key_down(KeyboardKey::LControl) {
                            terrain.clear_voxel(result.hit);
                        } else {
                            terrain.update_voxel(result.before_hit, default_material);
                        }
                    }
                }
                if ui_state.env.spawn_mode == SpawnMode::OneShot {
                    ui_state.env.spawning = false;
                }
            }
        }
    }

    #[profiling::function]
    pub fn update(&mut self, world: &mut World, resources: &mut Resources) {
        let mut terrain_resource = resources.get_mut::<TerrainResource>().unwrap();
        let mut storage = terrain_resource.write();
        let terrain = storage.get_mut(&self.terrain);
        terrain.update_chunks(world, resources);
    }

    pub fn spawn(
        &self,
        object_type: String,
        position: Point3i,
        resources: &Resources,
        world: &mut World,
    ) {
        // transform component
        let translation = Vec3::new(
            position.x() as f32,
            position.y() as f32,
            position.z() as f32,
        );
        let transform_component = TransformComponent {
            translation,
            scale: Vec3::ONE,
            rotation: Quat::IDENTITY,
        };

        let asset_manager = resources.get::<AssetManager>().unwrap();
        let handle = self.tiles.get(&object_type).unwrap().clone();
        let tile = asset_manager.committed_asset(&handle).unwrap().clone();

        // env object component
        let env_tile_component = EnvTileComponent {
            asset: handle,
            health: 1.,
            selected: false,
        };

        // entity
        log::info!("Spawn entity {:?} at: {}", object_type, translation);
        let _entity = world.push((transform_component, env_tile_component));

        // update voxels
        let mut terrain_resource = resources.get_mut::<TerrainResource>().unwrap();
        let mut storage = terrain_resource.write();
        let terrain = storage.get_mut(&self.terrain);
        terrain.instance_tile(&tile, position);
    }
}
