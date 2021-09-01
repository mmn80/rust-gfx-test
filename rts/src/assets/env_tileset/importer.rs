use std::io::Read;

use distill::{
    core::AssetUuid,
    importer::{ImportOp, ImportedAsset, Importer, ImporterValue},
};
use serde::{Deserialize, Serialize};
use type_uuid::*;

use crate::assets::env_tileset::EnvTileSetsAssetData;

#[derive(TypeUuid, Serialize, Deserialize, Default, Clone, Debug)]
#[uuid = "dbde9d62-6b8e-48a0-8bb9-61bf5a29c2f3"]
pub struct EnvTileSetsImporterStateStable {
    asset_uuid: Option<AssetUuid>,
}

impl From<EnvTileSetsImporterStateUnstable> for EnvTileSetsImporterStateStable {
    fn from(other: EnvTileSetsImporterStateUnstable) -> Self {
        let mut stable = EnvTileSetsImporterStateStable::default();
        stable.asset_uuid = other.asset_uuid.clone();
        stable
    }
}

#[derive(Default)]
pub struct EnvTileSetsImporterStateUnstable {
    asset_uuid: Option<AssetUuid>,
}

impl From<EnvTileSetsImporterStateStable> for EnvTileSetsImporterStateUnstable {
    fn from(other: EnvTileSetsImporterStateStable) -> Self {
        let mut unstable = EnvTileSetsImporterStateUnstable::default();
        unstable.asset_uuid = other.asset_uuid.clone();
        unstable
    }
}

#[derive(TypeUuid)]
#[uuid = "3963feb1-5168-4607-ac57-a0a5a9da957e"]
pub struct EnvTileSetsImporter;
impl Importer for EnvTileSetsImporter {
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
    type State = EnvTileSetsImporterStateStable;

    #[profiling::function]
    fn import(
        &self,
        _op: &mut ImportOp,
        source: &mut dyn Read,
        _options: &Self::Options,
        stable_state: &mut Self::State,
    ) -> distill::importer::Result<ImporterValue> {
        let mut imported_assets = Vec::<ImportedAsset>::default();

        let mut unstable_state: EnvTileSetsImporterStateUnstable = stable_state.clone().into();
        unstable_state.asset_uuid = Some(
            unstable_state
                .asset_uuid
                .unwrap_or_else(|| AssetUuid(*uuid::Uuid::new_v4().as_bytes())),
        );

        let asset_data = ron::de::from_reader::<_, EnvTileSetsAssetData>(source)?;

        imported_assets.push(ImportedAsset {
            id: unstable_state.asset_uuid.unwrap(),
            search_tags: vec![],
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
