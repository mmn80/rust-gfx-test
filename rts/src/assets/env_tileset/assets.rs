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

use crate::assets::env_tile::EnvTileAsset;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EnvTileSetTileHandle {
    pub name: String,
    pub handle: Handle<EnvTileAsset>,
}

#[derive(TypeUuid, Serialize, Deserialize, Debug, Clone)]
#[uuid = "ee0079f8-f37d-45b3-89cb-61048c59921d"]
pub struct EnvTileSetAssetData {
    pub name: String,
    pub tiles: Vec<EnvTileSetTileHandle>,
}

impl EnvTileSetAssetData {
    pub fn add_tile(&mut self, name: String, asset_resource: &AssetResource) {
        let file_name = name.to_lowercase().replace(" ", "_");
        let path = format!("tiles/{}.tile", file_name);
        let handle = asset_resource.load_asset_path::<EnvTileAsset, _>(path);
        self.tiles.push(EnvTileSetTileHandle { name, handle });
    }
}

pub struct EnvTileSetAssetInner {
    pub name: String,
    pub tiles: Vec<EnvTileSetTileHandle>,
}

#[derive(TypeUuid, Clone)]
#[uuid = "86051a83-f0d5-4256-b2a1-fbece8214dd1"]
pub struct EnvTileSetAsset {
    pub inner: Arc<EnvTileSetAssetInner>,
}

pub struct EnvTileSetLoadHandler;

impl DefaultAssetTypeLoadHandler<EnvTileSetAssetData, EnvTileSetAsset> for EnvTileSetLoadHandler {
    #[profiling::function]
    fn load(
        _asset_manager: &mut AssetManager,
        asset_data: EnvTileSetAssetData,
    ) -> RafxResult<EnvTileSetAsset> {
        Ok(EnvTileSetAsset {
            inner: Arc::new(EnvTileSetAssetInner {
                name: asset_data.name,
                tiles: asset_data.tiles,
            }),
        })
    }
}

pub type EnvTileSetAssetType =
    DefaultAssetTypeHandler<EnvTileSetAssetData, EnvTileSetAsset, EnvTileSetLoadHandler>;
