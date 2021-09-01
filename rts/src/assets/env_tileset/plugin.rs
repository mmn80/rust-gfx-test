use rafx::{
    assets::{distill_impl::AssetResource, AssetManager},
    distill::daemon::AssetDaemon,
    renderer::RendererAssetPlugin,
};

use super::{EnvTileSetsAssetType, EnvTileSetsImporter};

pub struct EnvTileSetsAssetTypeRendererPlugin;

impl RendererAssetPlugin for EnvTileSetsAssetTypeRendererPlugin {
    fn configure_asset_daemon(&self, asset_daemon: AssetDaemon) -> AssetDaemon {
        asset_daemon.with_importer("tilesets", EnvTileSetsImporter)
    }

    fn register_asset_types(
        &self,
        asset_manager: &mut AssetManager,
        asset_resource: &mut AssetResource,
    ) {
        asset_manager.register_asset_type::<EnvTileSetsAssetType>(asset_resource);
    }
}
