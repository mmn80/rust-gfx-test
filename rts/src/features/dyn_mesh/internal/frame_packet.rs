use glam::{Quat, Vec3};
use rafx::framework::{
    render_features::render_features_prelude::*, BufferResource, DescriptorSetArc,
    ImageViewResource, MaterialPassResource, ResourceArc,
};
use rafx_plugins::components::{
    DirectionalLightComponent, PointLightComponent, SpotLightComponent, TransformComponent,
};

use super::*;

pub struct DynMeshRenderFeatureTypes;

//---------
// EXTRACT
//---------

pub type DynMeshRenderObjectStaticData = DynMeshRenderObject;

pub struct DynMeshPerFrameData {
    pub depth_material_pass: Option<ResourceArc<MaterialPassResource>>,
}

pub struct DynMeshRenderObjectInstanceData {
    pub dyn_mesh: DynMesh,
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

#[derive(Default)]
pub struct DynMeshPerViewData {
    pub directional_lights: [Option<ExtractedDirectionalLight>; 16],
    pub point_lights: [Option<ExtractedPointLight>; 16],
    pub spot_lights: [Option<ExtractedSpotLight>; 16],
    pub num_directional_lights: u32,
    pub num_point_lights: u32,
    pub num_spot_lights: u32,
}

pub struct ExtractedDirectionalLight {
    pub light: DirectionalLightComponent,
    pub object_id: ObjectId,
}

pub struct ExtractedPointLight {
    pub light: PointLightComponent,
    pub transform: TransformComponent,
    pub object_id: ObjectId,
}

pub struct ExtractedSpotLight {
    pub light: SpotLightComponent,
    pub transform: TransformComponent,
    pub object_id: ObjectId,
}

impl FramePacketData for DynMeshRenderFeatureTypes {
    type PerFrameData = DynMeshPerFrameData;
    type RenderObjectInstanceData = Option<DynMeshRenderObjectInstanceData>;
    type PerViewData = DynMeshPerViewData;
    type RenderObjectInstancePerViewData = ();
}

pub type DynMeshFramePacket = FramePacket<DynMeshRenderFeatureTypes>;
pub type DynMeshViewPacket = ViewPacket<DynMeshRenderFeatureTypes>;

//---------
// PREPARE
//---------

//TODO: Pull this const from the shader
pub const MAX_SHADOW_MAPS_2D: usize = 32;
pub const MAX_SHADOW_MAPS_CUBE: usize = 16;

#[derive(Clone)]
pub struct DynMeshPartMaterialDescriptorSetPair {
    pub textured_descriptor_set: Option<DescriptorSetArc>,
    pub untextured_descriptor_set: Option<DescriptorSetArc>,
}

pub struct DynMeshPerFrameSubmitData {
    pub num_shadow_map_2d: usize,
    pub shadow_map_2d_data:
        [rafx_plugins::shaders::mesh_textured_frag::ShadowMap2DDataStd140; MAX_SHADOW_MAPS_2D],
    pub shadow_map_2d_image_views: [Option<ResourceArc<ImageViewResource>>; MAX_SHADOW_MAPS_2D],
    pub num_shadow_map_cube: usize,
    pub shadow_map_cube_data:
        [rafx_plugins::shaders::mesh_textured_frag::ShadowMapCubeDataStd140; MAX_SHADOW_MAPS_CUBE],
    pub shadow_map_cube_image_views: [Option<ResourceArc<ImageViewResource>>; MAX_SHADOW_MAPS_CUBE],
    pub shadow_map_image_index_remap: [Option<usize>; MAX_SHADOW_MAPS_2D + MAX_SHADOW_MAPS_CUBE],
    pub model_matrix_buffer: TrustCell<Option<ResourceArc<BufferResource>>>,
}

pub struct DynMeshRenderObjectInstanceSubmitData {
    pub model_matrix_offset: usize,
}

impl SubmitPacketData for DynMeshRenderFeatureTypes {
    type PerFrameSubmitData = Box<DynMeshPerFrameSubmitData>;
    type RenderObjectInstanceSubmitData = DynMeshRenderObjectInstanceSubmitData;
    type PerViewSubmitData = DynMeshPerViewSubmitData;
    type RenderObjectInstancePerViewSubmitData = ();
    type SubmitNodeData = DynMeshDrawCall;

    type RenderFeature = DynMeshRenderFeature;
}

pub type DynMeshSubmitPacket = SubmitPacket<DynMeshRenderFeatureTypes>;
pub type DynMeshViewSubmitPacket = ViewSubmitPacket<DynMeshRenderFeatureTypes>;

//-------
// WRITE
//-------

pub struct DynMeshPerViewSubmitData {
    pub opaque_descriptor_set: Option<DescriptorSetArc>,
    pub depth_descriptor_set: Option<DescriptorSetArc>,
}

pub struct DynMeshDrawCall {
    pub render_object_instance_id: RenderObjectInstanceId,
    pub material_pass_resource: ResourceArc<MaterialPassResource>,
    pub per_material_descriptor_set: Option<DescriptorSetArc>,
    pub mesh_part_index: usize,
    pub model_matrix_offset: usize,
}
