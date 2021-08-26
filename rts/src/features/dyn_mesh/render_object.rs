use rafx::render_features::RenderObjectSet;

use super::{DynMeshHandle, DynMeshRenderFeature};

#[derive(Clone)]
pub struct DynMeshRenderObject {
    pub mesh: DynMeshHandle,
}

pub type DynMeshRenderObjectSet = RenderObjectSet<DynMeshRenderFeature, DynMeshRenderObject>;
