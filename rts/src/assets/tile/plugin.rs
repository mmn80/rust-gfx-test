use rafx::{
    assets::{distill_impl::AssetResource, AssetManager},
    distill::daemon::AssetDaemon,
    renderer::RendererAssetPlugin,
};

use super::{EnvTileImporter, TileAssetType};

pub struct TileAssetTypeRendererPlugin;

impl RendererAssetPlugin for TileAssetTypeRendererPlugin {
    fn configure_asset_daemon(&self, asset_daemon: AssetDaemon) -> AssetDaemon {
        asset_daemon.with_importer(&["tile"], EnvTileImporter)
    }

    fn register_asset_types(
        &self,
        asset_manager: &mut AssetManager,
        asset_resource: &mut AssetResource,
    ) {
        asset_manager.register_asset_type::<TileAssetType>(asset_resource);
    }
}
