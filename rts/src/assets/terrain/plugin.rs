use super::{TerrainConfigAssetType, TerrainConfigImporter};
use rafx::{
    assets::{distill_impl::AssetResource, AssetManager},
    distill::daemon::AssetDaemon,
    renderer::RendererAssetPlugin,
};

pub struct TerrainConfigAssetTypeRendererPlugin;

impl RendererAssetPlugin for TerrainConfigAssetTypeRendererPlugin {
    fn configure_asset_daemon(&self, asset_daemon: AssetDaemon) -> AssetDaemon {
        asset_daemon.with_importer("terrainconfig", TerrainConfigImporter)
    }

    fn register_asset_types(
        &self,
        asset_manager: &mut AssetManager,
        asset_resource: &mut AssetResource,
    ) {
        asset_manager.register_asset_type::<TerrainConfigAssetType>(asset_resource);
    }
}
