use building_blocks::core::prelude::*;
use distill::loader::handle::Handle;
use glam::{Quat, Vec3};
use legion::{Entity, Resources};
use rafx::{
    assets::{distill_impl::AssetResource, AssetManager},
    renderer::ViewportsResource,
};
use rafx_plugins::components::{DirectionalLightComponent, TransformComponent};

use super::ui::{
    EnvUiCmd, TerrainEditUiState, TerrainResetUiState, TileEditUiState, TileSpawnUiState,
};
use crate::{
    assets::{
        pbr_material::PbrMaterialAsset,
        tile::{TileAsset, TileExporter},
        tilesets::{TileSetsAsset, TileSetsExportData, TileSetsExporter},
    },
    camera::RTSCamera,
    env::simulation::{Simulation, Universe, UniverseFillStyle},
    features::dyn_mesh::DynMeshManager,
    input::{InputResource, KeyboardKey, MouseButton},
    time::TimeState,
    ui::{SpawnMode, UiState},
    RenderOptions,
};

#[derive(Clone)]
pub struct TileComponent {
    pub asset: Handle<TileAsset>,
    pub health: f32,
    pub selected: bool,
}

const TILESETS_PATH: &str = "tiles/main.tilesets";

pub struct EnvState {
    main_light: Entity,
    tilesets: Handle<TileSetsAsset>,
}

impl EnvState {
    pub fn new(resources: &Resources, simulation: &mut Simulation) -> Self {
        let asset_resource = resources.get::<AssetResource>().unwrap();
        let tilesets = asset_resource.load_asset_path(TILESETS_PATH);
        {
            let material_names = Universe::get_default_material_names();
            let terrain_materials: Vec<_> = material_names
                .iter()
                .map(|name| {
                    let path = format!("materials/terrain/{}.pbrmaterial", *name);
                    let material_handle =
                        asset_resource.load_asset_path::<PbrMaterialAsset, _>(path);
                    (*name, material_handle.clone())
                })
                .collect();
            let dyn_mesh_manager = resources.get::<DynMeshManager>().unwrap();
            simulation.new_universe(
                &dyn_mesh_manager,
                terrain_materials,
                Point3i::ZERO,
                4096,
                UniverseFillStyle::FlatBoard {
                    material: "basic_tile",
                },
            );
        }
        let main_light = {
            let light_from = Vec3::new(0.0, 5.0, 4.0);
            let light_to = Vec3::ZERO;
            let light_direction = (light_to - light_from).normalize();
            let view_frustum = simulation
                .universe()
                .visibility_region
                .register_view_frustum();
            simulation
                .universe()
                .world
                .push((DirectionalLightComponent {
                    direction: light_direction,
                    intensity: 5.0,
                    color: [1.0, 1.0, 1.0, 1.0].into(),
                    view_frustum,
                },))
        };

        EnvState {
            main_light,
            tilesets,
        }
    }

    #[profiling::function]
    pub fn update(
        &mut self,
        simulation: &mut Simulation,
        resources: &mut Resources,
        ui_state: &mut UiState,
    ) {
        let universe = simulation.universe();
        {
            let input = resources.get::<InputResource>().unwrap();
            let time_state = resources.get::<TimeState>().unwrap();
            let mut viewports_resource = resources.get_mut::<ViewportsResource>().unwrap();
            let render_options = resources.get::<RenderOptions>().unwrap();
            let mut camera = resources.get_mut::<RTSCamera>().unwrap();

            camera.update(
                &*time_state,
                &*render_options,
                &mut universe.main_view_frustum,
                &mut *viewports_resource,
                &input,
            );
        }

        {
            if let Some(mut entry) = universe.world.entry(self.main_light) {
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

        universe.update_chunks(resources);
    }

    pub fn update_ui(
        &mut self,
        simulation: &mut Simulation,
        resources: &mut Resources,
        ui_state: &mut UiState,
        ui: &mut egui::Ui,
    ) {
        let tilesets = {
            let asset_manager = resources.get::<AssetManager>().unwrap();
            if let Some(asset) = asset_manager.committed_asset(&self.tilesets) {
                asset.clone()
            } else {
                ui.label("Waiting for tilesets asset to load...");
                return;
            }
        };
        let tilesets = {
            let mut asset_manager = resources.get_mut::<AssetManager>().unwrap();
            tilesets.get_loaded_tilesets(&mut asset_manager)
        };

        TileSpawnUiState::ui(ui_state, ui, &tilesets);
        if !ui_state.env.tile_spawn.active && !ui_state.unit.spawning {
            TileEditUiState::ui(ui_state, ui, &tilesets, |cmd| {
                self.ui_cmd_handler(cmd, simulation, resources)
            });
            TerrainEditUiState::ui(ui_state, ui);
            TerrainResetUiState::ui(ui_state, ui, |cmd| {
                self.ui_cmd_handler(cmd, simulation, resources)
            });
        }

        if ui_state.env.tile_spawn.active
            || (ui_state.env.terrain_edit.active && !ui_state.unit.spawning)
        {
            let input = resources.get::<InputResource>().unwrap();
            let camera = resources.get::<RTSCamera>().unwrap();
            let universe = simulation.universe();

            if input.is_mouse_just_down(MouseButton::LEFT) {
                let cursor_pos = input.mouse_position();
                let (cast_result, default_material) = {
                    let cast_result = camera.ray_cast_terrain(
                        cursor_pos.x as u32,
                        cursor_pos.y as u32,
                        universe,
                        ui_state,
                    );
                    let default_material = universe
                        .voxel_by_material(ui_state.env.terrain_edit.material)
                        .unwrap();
                    (cast_result, default_material)
                };
                if let Some(result) = cast_result {
                    if ui_state.env.tile_spawn.active {
                        self.spawn(
                            &ui_state.env.tile_spawn.tileset,
                            &ui_state.env.tile_spawn.tile,
                            PointN([result.hit.x(), result.hit.y(), result.hit.z() + 1]),
                            resources,
                            universe,
                        );
                    } else if ui_state.env.terrain_edit.active {
                        if input.is_key_down(KeyboardKey::LControl) {
                            universe.clear_voxel(result.hit);
                        } else {
                            universe.update_voxel(result.before_hit, default_material);
                        }
                    }
                }
                if ui_state.env.tile_spawn.mode == SpawnMode::OneShot {
                    ui_state.env.tile_spawn.active = false;
                }
            }
        }
    }

    fn ui_cmd_handler(
        &mut self,
        command: EnvUiCmd,
        simulation: &mut Simulation,
        resources: &mut Resources,
    ) -> Option<()> {
        let universe = simulation.universe();
        match command {
            EnvUiCmd::SaveEditedTile {
                tileset_name,
                tile_name,
            } => {
                {
                    universe.save_edited_tile(&tile_name)?;
                }
                if let Some(tileset_name) = tileset_name {
                    let tilesets = {
                        let asset_manager = resources.get::<AssetManager>().unwrap();
                        asset_manager
                            .committed_asset(&self.tilesets)
                            .unwrap()
                            .clone()
                    };
                    let tilesets = {
                        let mut asset_manager = resources.get_mut::<AssetManager>().unwrap();
                        tilesets.get_loaded_tilesets(&mut asset_manager)
                    };
                    let tilesets = TileSetsExportData::new(&tilesets, &tileset_name, &tile_name);
                    TileSetsExporter::export(&format!("assets/{}", TILESETS_PATH), tilesets)
                } else {
                    Some(())
                }
            }
            EnvUiCmd::StartEditTile {
                tileset_name,
                tile_name,
            } => {
                {
                    let terrain_style = UniverseFillStyle::FlatBoard {
                        material: "basic_tile",
                    };
                    universe.reset(Point3i::ZERO, TILE_EDIT_PLATFORM_SIZE as u32, terrain_style);
                }
                if !tile_name.is_empty() {
                    self.spawn(
                        &tileset_name,
                        &tile_name,
                        Point3i::ZERO,
                        resources,
                        universe,
                    );
                }
                Some(())
            }
            EnvUiCmd::ResetTerrain(params) => {
                universe.reset(Point3i::ZERO, params.size, params.style.clone());
                Some(())
            }
        }
    }

    pub fn spawn(
        &self,
        tileset_name: &str,
        tile_name: &str,
        position: Point3i,
        resources: &Resources,
        universe: &mut Universe,
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

        // tile component
        let tile_component = TileComponent {
            asset: {
                let asset_resource = resources.get::<AssetResource>().unwrap();
                asset_resource.load_asset_path(TileExporter::get_tile_path(tile_name, false))
            },
            health: 1.,
            selected: false,
        };

        // entity
        log::info!("Spawn tile {} at: {}", tile_name, translation);
        let _entity = universe.world.push((transform_component, tile_component));

        // update voxels
        let tile = {
            let tilesets = {
                let asset_manager = resources.get::<AssetManager>().unwrap();
                asset_manager
                    .committed_asset(&self.tilesets)
                    .unwrap()
                    .clone()
            };
            let tilesets = {
                let mut asset_manager = resources.get_mut::<AssetManager>().unwrap();
                tilesets.get_loaded_tilesets(&mut asset_manager)
            };
            let tileset = tilesets
                .iter()
                .find(|tileset| &tileset.name == tileset_name)
                .unwrap();
            tileset
                .tiles
                .iter()
                .find(|tile| &tile.inner.name == tile_name)
                .unwrap()
                .clone()
        };

        universe.instance_tile(&tile, position);
    }
}

const TILE_EDIT_PLATFORM_SIZE: i32 = 32;
