use rafx::{
    assets::{distill_impl::AssetResource, AssetManager},
    distill::daemon::AssetDaemon,
    renderer::RendererAssetPlugin,
};

use super::{EnvTileAssetType, EnvTileImporter};

pub struct EnvTileAssetTypeRendererPlugin;

impl RendererAssetPlugin for EnvTileAssetTypeRendererPlugin {
    fn configure_asset_daemon(&self, asset_daemon: AssetDaemon) -> AssetDaemon {
        asset_daemon.with_importer("tile", EnvTileImporter)
    }

    fn register_asset_types(
        &self,
        asset_manager: &mut AssetManager,
        asset_resource: &mut AssetResource,
    ) {
        asset_manager.register_asset_type::<EnvTileAssetType>(asset_resource);
    }
}