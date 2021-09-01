use super::EnvTileSetsAssetData;

// don't know how to do it from distill
pub struct EnvTileSetsExporter;

impl EnvTileSetsExporter {
    pub fn export(path: String, asset_data: &EnvTileSetsAssetData) -> Option<()> {
        let asset_string =
            ron::ser::to_string_pretty::<EnvTileSetsAssetData>(asset_data, Default::default())
                .ok()?;
        std::fs::write(path, asset_string).ok()
    }
}
