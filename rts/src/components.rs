use crate::features::mesh::MeshRenderNodeHandle;
use glam::f32::Vec3;
use glam::Quat;
use rafx::framework::visibility::VisibilityObjectArc;
use rafx::visibility::ViewFrustumArc;

#[derive(Clone)]
pub struct MeshComponent {
    pub render_node: MeshRenderNodeHandle,
}

#[derive(Clone)]
pub struct VisibilityComponent {
    pub handle: VisibilityObjectArc,
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

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum UnitType {
    Container1,
    Container2,
    BlueIcosphere,
}

#[derive(Clone)]
pub struct UnitComponent {
    pub unit_type: UnitType,
    pub health: f32,
    pub aim: Vec3,
    pub speed: f32,
    pub move_target: Option<Vec3>,
}
