use super::{PbrMaterialAssetType, PbrMaterialImporter};
use rafx::{
    assets::{distill_impl::AssetResource, AssetManager},
    distill::daemon::AssetDaemon,
    renderer::RendererAssetPlugin,
};

pub struct PbrMaterialAssetTypeRendererPlugin;

impl RendererAssetPlugin for PbrMaterialAssetTypeRendererPlugin {
    fn configure_asset_daemon(&self, asset_daemon: AssetDaemon) -> AssetDaemon {
        asset_daemon.with_importer("pbrmaterial", PbrMaterialImporter)
    }

    fn register_asset_types(
        &self,
        asset_manager: &mut AssetManager,
        asset_resource: &mut AssetResource,
    ) {
        asset_manager.register_asset_type::<PbrMaterialAssetType>(asset_resource);
    }
}
