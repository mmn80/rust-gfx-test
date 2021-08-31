use self::terrain::TerrainFillStyle;
use crate::ui::SpawnMode;

pub mod env_object;
pub mod perlin;
pub mod terrain;

pub struct EnvUiState {
    pub spawning: bool,
    pub spawn_mode: SpawnMode,
    pub spawn_tile: String,
    pub edit_tile: Option<String>,
    pub edit_new_tile: bool,
    pub edit_mode: bool,
    pub edit_material: &'static str,
    pub terrain_size: u32,
    pub terrain_style: TerrainFillStyle,
}

impl Default for EnvUiState {
    fn default() -> Self {
        Self {
            spawning: false,
            spawn_mode: SpawnMode::OneShot,
            spawn_tile: "Bilding".to_string(),
            edit_tile: None,
            edit_new_tile: false,
            edit_mode: false,
            edit_material: "basic_tile",
            terrain_size: 4096,
            terrain_style: TerrainFillStyle::FlatBoard {
                material: "basic_tile",
            },
        }
    }
}
