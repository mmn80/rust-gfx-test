use std::io::Read;

use distill::{
    core::AssetUuid,
    importer::{ImportOp, ImportedAsset, Importer, ImporterValue},
};
use serde::{Deserialize, Serialize};
use type_uuid::*;

use crate::assets::tile::TileAssetData;

#[derive(TypeUuid, Serialize, Deserialize, Default, Clone, Debug)]
#[uuid = "6b5e8cc4-9a8e-45e2-9e25-7f1ab02f4ca0"]
pub struct EnvTileImporterStateStable {
    tile_asset_uuid: Option<AssetUuid>,
}

impl From<EnvTileImporterStateUnstable> for EnvTileImporterStateStable {
    fn from(other: EnvTileImporterStateUnstable) -> Self {
        let mut stable = EnvTileImporterStateStable::default();
        stable.tile_asset_uuid = other.tile_asset_uuid.clone();
        stable
    }
}

#[derive(Default)]
pub struct EnvTileImporterStateUnstable {
    tile_asset_uuid: Option<AssetUuid>,
}

impl From<EnvTileImporterStateStable> for EnvTileImporterStateUnstable {
    fn from(other: EnvTileImporterStateStable) -> Self {
        let mut unstable = EnvTileImporterStateUnstable::default();
        unstable.tile_asset_uuid = other.tile_asset_uuid.clone();
        unstable
    }
}

#[derive(TypeUuid)]
#[uuid = "39fd954d-2463-4e89-907b-b0f12cb3abd7"]
pub struct EnvTileImporter;
impl Importer for EnvTileImporter {
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
    type State = EnvTileImporterStateStable;

    #[profiling::function]
    fn import(
        &self,
        _op: &mut ImportOp,
        source: &mut dyn Read,
        _options: &Self::Options,
        stable_state: &mut Self::State,
    ) -> distill::importer::Result<ImporterValue> {
        let mut imported_assets = Vec::<ImportedAsset>::default();

        let mut unstable_state: EnvTileImporterStateUnstable = stable_state.clone().into();
        unstable_state.tile_asset_uuid = Some(
            unstable_state
                .tile_asset_uuid
                .unwrap_or_else(|| AssetUuid(*uuid::Uuid::new_v4().as_bytes())),
        );

        let asset_data = ron::de::from_reader::<_, TileAssetData>(source)?;

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
