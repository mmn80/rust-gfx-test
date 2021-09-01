use rafx::{
    assets::{distill_impl::AssetResource, AssetManager},
    distill::daemon::AssetDaemon,
    renderer::RendererAssetPlugin,
};

use super::{TileSetsAssetType, TileSetsImporter};

pub struct TileSetsAssetTypeRendererPlugin;

impl RendererAssetPlugin for TileSetsAssetTypeRendererPlugin {
    fn configure_asset_daemon(&self, asset_daemon: AssetDaemon) -> AssetDaemon {
        asset_daemon.with_importer("tilesets", TileSetsImporter)
    }

    fn register_asset_types(
        &self,
        asset_manager: &mut AssetManager,
        asset_resource: &mut AssetResource,
    ) {
        asset_manager.register_asset_type::<TileSetsAssetType>(asset_resource);
    }
}
