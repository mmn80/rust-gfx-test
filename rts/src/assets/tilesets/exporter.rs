use serde::{Deserialize, Serialize};

use crate::assets::tile::TileExporter;

use super::LoadedTileSet;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TileSetExportData {
    pub name: String,
    pub tiles: Vec<String>,
}

impl TileSetExportData {
    pub fn new(tileset: &LoadedTileSet) -> Self {
        Self {
            name: tileset.name.clone(),
            tiles: tileset
                .tiles
                .iter()
                .map(|tile| TileExporter::get_tile_file_name(&tile.inner.name))
                .collect(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TileSetsExportData {
    pub tilesets: Vec<TileSetExportData>,
}

impl TileSetsExportData {
    pub fn new(tilesets: &Vec<LoadedTileSet>, new_tile_tileset: &str, new_tile: &str) -> Self {
        Self {
            tilesets: tilesets
                .iter()
                .map(|tileset| {
                    let mut tileset = TileSetExportData::new(tileset);
                    if tileset.name == new_tile_tileset {
                        tileset
                            .tiles
                            .push(TileExporter::get_tile_file_name(new_tile));
                    }
                    tileset
                })
                .collect(),
        }
    }
}

pub struct TileSetsExporter;

impl TileSetsExporter {
    pub fn export(path: &str, asset_data: TileSetsExportData) -> Option<()> {
        let asset_string =
            ron::ser::to_string_pretty::<TileSetsExportData>(&asset_data, Default::default())
                .ok()?;
        std::fs::write(path, asset_string).ok()
    }
}
