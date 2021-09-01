use rafx::{
    assets::{distill_impl::AssetResource, AssetManager},
    distill::daemon::AssetDaemon,
    renderer::RendererAssetPlugin,
};

use super::{EnvTileSetAssetType, EnvTileSetImporter};

pub struct EnvTileSetAssetTypeRendererPlugin;

impl RendererAssetPlugin for EnvTileSetAssetTypeRendererPlugin {
    fn configure_asset_daemon(&self, asset_daemon: AssetDaemon) -> AssetDaemon {
        asset_daemon.with_importer("tileset", EnvTileSetImporter)
    }

    fn register_asset_types(
        &self,
        asset_manager: &mut AssetManager,
        asset_resource: &mut AssetResource,
    ) {
        asset_manager.register_asset_type::<EnvTileSetAssetType>(asset_resource);
    }
}
