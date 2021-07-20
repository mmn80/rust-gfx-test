use super::{DynMeshHandle, DynMeshRenderFeature};
use rafx::render_features::RenderObjectSet;

#[derive(Clone)]
pub struct DynMeshRenderObject {
    pub mesh: DynMeshHandle,
}

pub type DynMeshRenderObjectSet = RenderObjectSet<DynMeshRenderFeature, DynMeshRenderObject>;
