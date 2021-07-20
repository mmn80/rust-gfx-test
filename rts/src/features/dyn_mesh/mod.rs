mod jobs;
use jobs::*;
mod internal;
use internal::*;

use rafx::render_feature_mod_prelude::*;
rafx::declare_render_feature!(DynMeshRenderFeature, DYN_MESH_FEATURE_INDEX);

rafx::declare_render_feature_flag!(
    DynMeshWireframeRenderFeatureFlag,
    DYN_MESH_WIREFRAME_FLAG_INDEX
);

rafx::declare_render_feature_flag!(
    DynMeshUntexturedRenderFeatureFlag,
    MESH_UNTEXTURED_FLAG_INDEX
);

rafx::declare_render_feature_flag!(DynMeshUnlitRenderFeatureFlag, DYN_MESH_UNLIT_FLAG_INDEX);

rafx::declare_render_feature_flag!(
    DynMeshNoShadowsRenderFeatureFlag,
    DYN_MESH_NO_SHADOWS_FLAG_INDEX
);

// Public API

mod plugin;
pub use plugin::*;

pub use jobs::DynMeshVertex;

mod dyn_mesh;
pub use dyn_mesh::*;

mod render_object;
pub use render_object::*;
