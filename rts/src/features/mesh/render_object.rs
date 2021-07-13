use super::{internal::ExtractedMesh, DynMeshCache, DynMeshHandle, MeshRenderFeature};
use crate::assets::mesh::MeshAsset;
use distill::loader::handle::Handle;
use rafx::{render_feature_renderer_prelude::AssetManager, render_features::RenderObjectSet};

#[derive(Clone)]
pub enum MeshRenderObject {
    AssetHandle(Handle<MeshAsset>),
    DynMeshHandle(DynMeshHandle),
}

impl MeshRenderObject {
    pub fn extract_mesh(
        &self,
        dyn_mesh_cache: &DynMeshCache,
        asset_manager: &AssetManager,
    ) -> Option<ExtractedMesh> {
        match self {
            MeshRenderObject::AssetHandle(asset_handle) => asset_manager
                .committed_asset(&asset_handle)
                .and_then(|asset| Some(ExtractedMesh::MeshAsset(asset.clone()))),
            MeshRenderObject::DynMeshHandle(dyn_mesh_handle) => Some(ExtractedMesh::DynMesh(
                dyn_mesh_cache.get_dyn_mesh(dyn_mesh_handle.clone()),
            )),
        }
    }
}

pub type MeshRenderObjectSet = RenderObjectSet<MeshRenderFeature, MeshRenderObject>;
