use super::EnvTileSetAssetData;

// don't know how to do it from distill
pub struct EnvTileSetExporter;

impl EnvTileSetExporter {
    pub fn export(name: String, asset_data: &EnvTileSetAssetData) -> Option<()> {
        let asset_string =
            ron::ser::to_string_pretty::<EnvTileSetAssetData>(asset_data, Default::default())
                .ok()?;
        let file_name = name.to_lowercase().replace(" ", "_");
        let path = format!("assets/tiles/{}.tileset", file_name);
        std::fs::write(path, asset_string).ok()
    }
}
