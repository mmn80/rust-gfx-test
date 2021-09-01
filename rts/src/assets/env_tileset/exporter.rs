use super::EnvTileSetsAssetData;

// don't know how to do it from distill
pub struct EnvTileSetsExporter;

impl EnvTileSetsExporter {
    pub fn export(path: &str, asset_data: EnvTileSetsAssetData) -> Option<()> {
        log::info!("Generating string");
        let asset_string =
            ron::ser::to_string_pretty::<EnvTileSetsAssetData>(&asset_data, Default::default())
                .ok()?;
        log::info!("Savinbg file");
        std::fs::write(path, asset_string).ok()?;
        log::info!("Saved");
        Some(())
    }
}
