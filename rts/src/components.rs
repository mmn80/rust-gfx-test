use crate::terrain::TerrainHandle;
use glam::{f32::Vec3, Quat};
use rafx::{
    framework::visibility::VisibilityObjectArc, render_features::RenderObjectHandle,
    visibility::ViewFrustumArc,
};

#[derive(Clone)]
pub struct MeshComponent {
    pub render_object_handle: RenderObjectHandle,
}

#[derive(Clone)]
pub struct VisibilityComponent {
    pub visibility_object_handle: VisibilityObjectArc,
}

#[derive(Clone)]
pub struct TerrainComponent {
    pub handle: TerrainHandle,
}

#[derive(Clone, Copy)]
pub struct TransformComponent {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Default for TransformComponent {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

#[derive(Clone)]
pub struct PointLightComponent {
    pub color: glam::Vec4,
    pub range: f32,
    pub intensity: f32,
    pub view_frustums: [ViewFrustumArc; 6],
}

#[derive(Clone)]
pub struct DirectionalLightComponent {
    pub direction: Vec3,
    pub color: glam::Vec4,
    pub intensity: f32,
    pub view_frustum: ViewFrustumArc,
}

#[derive(Clone)]
pub struct SpotLightComponent {
    pub direction: Vec3,
    pub color: glam::Vec4,
    pub spotlight_half_angle: f32,
    pub range: f32,
    pub intensity: f32,
    pub view_frustum: ViewFrustumArc,
}
