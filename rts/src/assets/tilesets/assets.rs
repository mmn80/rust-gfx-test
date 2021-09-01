use std::sync::Arc;

use distill::loader::handle::Handle;
use rafx::{
    api::RafxResult,
    assets::{AssetManager, DefaultAssetTypeHandler, DefaultAssetTypeLoadHandler},
};
use serde::{Deserialize, Serialize};
use type_uuid::*;

use crate::assets::tile::TileAsset;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TileSet {
    pub name: String,
    pub tiles: Vec<Handle<TileAsset>>,
}

#[derive(TypeUuid, Serialize, Deserialize, Debug, Clone)]
#[uuid = "ee0079f8-f37d-45b3-89cb-61048c59921d"]
pub struct TileSetsAssetData {
    pub tilesets: Vec<TileSet>,
}

#[derive(TypeUuid, Clone)]
#[uuid = "86051a83-f0d5-4256-b2a1-fbece8214dd1"]
pub struct TileSetsAsset {
    pub tilesets: Arc<Vec<TileSet>>,
}

pub struct LoadedTileSet {
    pub name: String,
    pub tiles: Vec<TileAsset>,
}

impl TileSetsAsset {
    pub fn get_loaded_tilesets(&self, asset_manager: &mut AssetManager) -> Vec<LoadedTileSet> {
        self.tilesets
            .iter()
            .map(|tileset| LoadedTileSet {
                name: tileset.name.clone(),
                tiles: tileset
                    .tiles
                    .iter()
                    .map(|handle| asset_manager.committed_asset(handle).unwrap().clone())
                    .collect(),
            })
            .collect()
    }
}

pub struct TileSetsLoadHandler;

impl DefaultAssetTypeLoadHandler<TileSetsAssetData, TileSetsAsset> for TileSetsLoadHandler {
    #[profiling::function]
    fn load(
        _asset_manager: &mut AssetManager,
        asset_data: TileSetsAssetData,
    ) -> RafxResult<TileSetsAsset> {
        Ok(TileSetsAsset {
            tilesets: Arc::new(asset_data.tilesets),
        })
    }
}

pub type TileSetsAssetType =
    DefaultAssetTypeHandler<TileSetsAssetData, TileSetsAsset, TileSetsLoadHandler>;
