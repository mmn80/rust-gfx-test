use serde::{Deserialize, Serialize};

use crate::assets::env_tile::EnvTileExporter;

use super::LoadedEnvTileSet;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EnvTileSetExportData {
    pub name: String,
    pub tiles: Vec<String>,
}

impl EnvTileSetExportData {
    pub fn new(tileset: &LoadedEnvTileSet) -> Self {
        Self {
            name: tileset.name.clone(),
            tiles: tileset
                .tiles
                .iter()
                .map(|tile| EnvTileExporter::get_tile_file_name(&tile.inner.name))
                .collect(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EnvTileSetsExportData {
    pub tilesets: Vec<EnvTileSetExportData>,
}

impl EnvTileSetsExportData {
    pub fn new(tilesets: &Vec<LoadedEnvTileSet>, new_tile_tileset: &str, new_tile: &str) -> Self {
        Self {
            tilesets: tilesets
                .iter()
                .map(|tileset| {
                    let mut tileset = EnvTileSetExportData::new(tileset);
                    if tileset.name == new_tile_tileset {
                        tileset
                            .tiles
                            .push(EnvTileExporter::get_tile_file_name(new_tile));
                    }
                    tileset
                })
                .collect(),
        }
    }
}

pub struct EnvTileSetsExporter;

impl EnvTileSetsExporter {
    pub fn export(path: &str, asset_data: EnvTileSetsExportData) -> Option<()> {
        let asset_string =
            ron::ser::to_string_pretty::<EnvTileSetsExportData>(&asset_data, Default::default())
                .ok()?;
        std::fs::write(path, asset_string).ok()
    }
}
