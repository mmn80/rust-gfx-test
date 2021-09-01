use std::sync::Arc;

use distill::loader::handle::Handle;
use rafx::{
    api::RafxResult,
    assets::{
        distill_impl::AssetResource, AssetManager, DefaultAssetTypeHandler,
        DefaultAssetTypeLoadHandler,
    },
};
use serde::{Deserialize, Serialize};
use type_uuid::*;

use crate::assets::env_tile::{EnvTileAsset, EnvTileExporter};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EnvTileSet {
    pub name: String,
    pub tiles: Vec<Handle<EnvTileAsset>>,
}

impl EnvTileSet {
    pub fn add_tile(&mut self, tile_name: &str, asset_resource: &AssetResource) {
        let handle = asset_resource
            .load_asset_path::<EnvTileAsset, _>(EnvTileExporter::get_tile_path(tile_name, false));
        self.tiles.push(handle);
    }
}

#[derive(TypeUuid, Serialize, Deserialize, Debug, Clone)]
#[uuid = "ee0079f8-f37d-45b3-89cb-61048c59921d"]
pub struct EnvTileSetsAssetData {
    pub tilesets: Vec<EnvTileSet>,
}

pub struct EnvTileSetsAssetInner {
    pub tilesets: Vec<EnvTileSet>,
}

#[derive(TypeUuid, Clone)]
#[uuid = "86051a83-f0d5-4256-b2a1-fbece8214dd1"]
pub struct EnvTileSetsAsset {
    pub inner: Arc<EnvTileSetsAssetInner>,
}

pub struct LoadedEnvTileSet {
    pub name: String,
    pub tiles: Vec<EnvTileAsset>,
}

impl EnvTileSetsAsset {
    pub fn get_loaded_tilesets(&self, asset_manager: &mut AssetManager) -> Vec<LoadedEnvTileSet> {
        self.inner
            .tilesets
            .iter()
            .map(|tileset| LoadedEnvTileSet {
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

pub struct EnvTileSetsLoadHandler;

impl DefaultAssetTypeLoadHandler<EnvTileSetsAssetData, EnvTileSetsAsset>
    for EnvTileSetsLoadHandler
{
    #[profiling::function]
    fn load(
        _asset_manager: &mut AssetManager,
        asset_data: EnvTileSetsAssetData,
    ) -> RafxResult<EnvTileSetsAsset> {
        Ok(EnvTileSetsAsset {
            inner: Arc::new(EnvTileSetsAssetInner {
                tilesets: asset_data.tilesets,
            }),
        })
    }
}

pub type EnvTileSetsAssetType =
    DefaultAssetTypeHandler<EnvTileSetsAssetData, EnvTileSetsAsset, EnvTileSetsLoadHandler>;
