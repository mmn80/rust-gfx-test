use building_blocks::core::prelude::*;
use distill::loader::handle::Handle;
use glam::{Quat, Vec3};
use legion::{Resources, World};
use rafx::assets::{distill_impl::AssetResource, AssetManager};
use rafx_plugins::components::TransformComponent;

use super::ui::{
    EnvUiCmd, TerrainEditUiState, TerrainResetUiState, TileEditUiState, TileSpawnUiState,
};
use crate::{
    assets::{
        env_tile::EnvTileAsset, env_tileset::EnvTileSetAsset, pbr_material::PbrMaterialAsset,
    },
    camera::RTSCamera,
    env::terrain::{Terrain, TerrainFillStyle, TerrainHandle, TerrainResource},
    input::{InputResource, KeyboardKey, MouseButton},
    ui::{SpawnMode, UiState},
};

#[derive(Clone)]
pub struct EnvTileComponent {
    pub asset: Handle<EnvTileAsset>,
    pub health: f32,
    pub selected: bool,
}

const TILESETS: [&str; 2] = ["Base", "Extra"];

pub struct EnvObjectsState {
    pub terrain: TerrainHandle,
    tilesets: Vec<Handle<EnvTileSetAsset>>,
}

impl EnvObjectsState {
    pub fn new(resources: &Resources, world: &mut World) -> Self {
        let asset_resource = resources.get::<AssetResource>().unwrap();

        let material_names = Terrain::get_default_material_names();
        let terrain_materials: Vec<_> = material_names
            .iter()
            .map(|name| {
                let path = format!("materials/terrain/{}.pbrmaterial", *name);
                let material_handle = asset_resource.load_asset_path::<PbrMaterialAsset, _>(path);
                (*name, material_handle.clone())
            })
            .collect();

        let tilesets = TILESETS
            .iter()
            .map(|name| {
                let file_name = name.to_lowercase().replace(" ", "_");
                let path = format!("tiles/{}.tileset", file_name);
                let tile_handle = asset_resource.load_asset_path::<EnvTileSetAsset, _>(path);
                tile_handle.clone()
            })
            .collect();

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

        EnvObjectsState { terrain, tilesets }
    }

    #[profiling::function]
    pub fn update(&mut self, world: &mut World, resources: &mut Resources) {
        let mut terrain_resource = resources.get_mut::<TerrainResource>().unwrap();
        let mut storage = terrain_resource.write();
        let terrain = storage.get_mut(&self.terrain);
        terrain.update_chunks(world, resources);
    }

    pub fn update_ui(
        &mut self,
        world: &mut World,
        resources: &mut Resources,
        ui_state: &mut UiState,
        ui: &mut egui::Ui,
    ) {
        TileSpawnUiState::ui(ui_state, ui, &self.get_loaded_tilesets(resources));
        if !ui_state.env.spawn_tile.spawning && !ui_state.unit.spawning {
            TileEditUiState::ui(ui_state, ui, &self.get_loaded_tilesets(resources), |cmd| {
                self.ui_cmd_handler(cmd, world, resources)
            });
            TerrainEditUiState::ui(ui_state, ui);
            TerrainResetUiState::ui(ui_state, ui, |cmd| {
                self.ui_cmd_handler(cmd, world, resources)
            });
        }

        if ui_state.env.spawn_tile.spawning
            || (ui_state.env.edit_terrain.edit_mode && !ui_state.unit.spawning)
        {
            let input = resources.get::<InputResource>().unwrap();
            let camera = resources.get::<RTSCamera>().unwrap();

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
                        .voxel_by_material(ui_state.env.edit_terrain.edit_material)
                        .unwrap();
                    (cast_result, default_material)
                };
                if let Some(result) = cast_result {
                    if ui_state.env.spawn_tile.spawning {
                        self.spawn(
                            &ui_state.env.spawn_tile.spawn_tileset,
                            &ui_state.env.spawn_tile.spawn_tile,
                            PointN([result.hit.x(), result.hit.y(), result.hit.z() + 1]),
                            resources,
                            world,
                        );
                    } else if ui_state.env.edit_terrain.edit_mode {
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
                if ui_state.env.spawn_tile.spawn_mode == SpawnMode::OneShot {
                    ui_state.env.spawn_tile.spawning = false;
                }
            }
        }
    }

    fn ui_cmd_handler(
        &mut self,
        command: EnvUiCmd,
        world: &mut World,
        resources: &mut Resources,
    ) -> Option<()> {
        match command {
            EnvUiCmd::SaveEditedTile(tile) => {
                let mut terrain_resource = resources.get_mut::<TerrainResource>().unwrap();
                let mut storage = terrain_resource.write();
                let terrain = storage.get_mut(&self.terrain);
                terrain.save_edited_tile(&tile)
            }
            EnvUiCmd::StartEditTile {
                tileset_name,
                tile_name,
            } => {
                {
                    let mut terrain_resource = resources.get_mut::<TerrainResource>().unwrap();
                    let mut storage = terrain_resource.write();
                    let terrain = storage.get_mut(&self.terrain);
                    let terrain_style = TerrainFillStyle::FlatBoard {
                        material: "basic_tile",
                    };
                    terrain.reset(
                        world,
                        Point3i::ZERO,
                        TILE_EDIT_PLATFORM_SIZE as u32,
                        terrain_style,
                    );
                }
                if !tile_name.is_empty() {
                    self.spawn(&tileset_name, &tile_name, Point3i::ZERO, resources, world);
                }
                Some(())
            }
            EnvUiCmd::ResetTerrain(params) => {
                let mut terrain_resource = resources.get_mut::<TerrainResource>().unwrap();
                let mut storage = terrain_resource.write();
                let terrain = storage.get_mut(&self.terrain);
                terrain.reset(
                    world,
                    Point3i::ZERO,
                    params.terrain_size,
                    params.terrain_style.clone(),
                );
                Some(())
            }
        }
    }

    fn get_loaded_tilesets(&self, resources: &Resources) -> Vec<EnvTileSetAsset> {
        let asset_manager = resources.get::<AssetManager>().unwrap();
        self.tilesets
            .iter()
            .map(|tileset| asset_manager.committed_asset(tileset).unwrap().clone())
            .collect()
    }

    pub fn spawn(
        &self,
        tileset_name: &str,
        tile_name: &str,
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

        let tilesets = self.get_loaded_tilesets(resources);
        let asset_manager = resources.get::<AssetManager>().unwrap();
        let tileset = tilesets
            .iter()
            .find(|tileset| &tileset.inner.name == &tileset_name)
            .unwrap();
        let tile_handle = tileset.get_tile_handle(tile_name);
        let tile = asset_manager.committed_asset(&tile_handle).unwrap();

        // env object component
        let env_tile_component = EnvTileComponent {
            asset: tile_handle,
            health: 1.,
            selected: false,
        };

        // entity
        log::info!("Spawn tile {:?} at: {}", tile_name, translation);
        let _entity = world.push((transform_component, env_tile_component));

        // update voxels
        let mut terrain_resource = resources.get_mut::<TerrainResource>().unwrap();
        let mut storage = terrain_resource.write();
        let terrain = storage.get_mut(&self.terrain);
        terrain.instance_tile(&tile, position);
    }
}

const TILE_EDIT_PLATFORM_SIZE: i32 = 32;
