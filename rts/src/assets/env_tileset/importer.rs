use std::io::Read;

use distill::{
    core::AssetUuid,
    importer::{ImportOp, ImportedAsset, Importer, ImporterValue},
};
use serde::{Deserialize, Serialize};
use type_uuid::*;

use crate::assets::env_tileset::EnvTileSetAssetData;

#[derive(TypeUuid, Serialize, Deserialize, Default, Clone, Debug)]
#[uuid = "dbde9d62-6b8e-48a0-8bb9-61bf5a29c2f3"]
pub struct EnvTileSetImporterStateStable {
    tileset_asset_uuid: Option<AssetUuid>,
}

impl From<EnvTileSetImporterStateUnstable> for EnvTileSetImporterStateStable {
    fn from(other: EnvTileSetImporterStateUnstable) -> Self {
        let mut stable = EnvTileSetImporterStateStable::default();
        stable.tileset_asset_uuid = other.tile_asset_uuid.clone();
        stable
    }
}

#[derive(Default)]
pub struct EnvTileSetImporterStateUnstable {
    tile_asset_uuid: Option<AssetUuid>,
}

impl From<EnvTileSetImporterStateStable> for EnvTileSetImporterStateUnstable {
    fn from(other: EnvTileSetImporterStateStable) -> Self {
        let mut unstable = EnvTileSetImporterStateUnstable::default();
        unstable.tile_asset_uuid = other.tileset_asset_uuid.clone();
        unstable
    }
}

#[derive(TypeUuid)]
#[uuid = "3963feb1-5168-4607-ac57-a0a5a9da957e"]
pub struct EnvTileSetImporter;
impl Importer for EnvTileSetImporter {
    fn version_static() -> u32
    where
        Self: Sized,
    {
        1
    }

    fn version(&self) -> u32 {
        Self::version_static()
    }

    type Options = ();
    type State = EnvTileSetImporterStateStable;

    #[profiling::function]
    fn import(
        &self,
        _op: &mut ImportOp,
        source: &mut dyn Read,
        _options: &Self::Options,
        stable_state: &mut Self::State,
    ) -> distill::importer::Result<ImporterValue> {
        let mut imported_assets = Vec::<ImportedAsset>::default();

        let mut unstable_state: EnvTileSetImporterStateUnstable = stable_state.clone().into();
        unstable_state.tile_asset_uuid = Some(
            unstable_state
                .tile_asset_uuid
                .unwrap_or_else(|| AssetUuid(*uuid::Uuid::new_v4().as_bytes())),
        );

        let asset_data = ron::de::from_reader::<_, EnvTileSetAssetData>(source)?;

        let mut search_tags: Vec<(String, Option<String>)> = vec![];
        search_tags.push(("name".to_string(), Some(asset_data.name.clone())));

        imported_assets.push(ImportedAsset {
            id: unstable_state.tile_asset_uuid.unwrap(),
            search_tags,
            build_deps: vec![],
            load_deps: vec![],
            build_pipeline: None,
            asset_data: Box::new(asset_data),
        });

        *stable_state = unstable_state.into();

        Ok(ImporterValue {
            assets: imported_assets,
        })
    }
}
