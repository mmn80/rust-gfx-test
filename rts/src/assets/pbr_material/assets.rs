use distill::loader::handle::Handle;
use rafx::{
    api::{RafxError, RafxResult},
    assets::{
        AssetManager, DefaultAssetTypeHandler, DefaultAssetTypeLoadHandler, ImageAsset,
        MaterialInstanceAsset,
    },
};
use rafx_plugins::assets::mesh::MeshMaterialDataShaderParam;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use type_uuid::*;

#[derive(TypeUuid, Serialize, Deserialize, Debug, Clone)]
#[uuid = "df939dee-7ff0-496d-ae45-11ffbe268e4f"]
pub struct PbrMaterialSource {
    pub name: String,

    pub base_color_factor: [f32; 4],
    pub emissive_factor: [f32; 3],
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub normal_texture_scale: f32,
    pub occlusion_texture_strength: f32,
    pub alpha_cutoff: f32,

    pub base_color_texture: Option<Handle<ImageAsset>>,
    pub metallic_roughness_texture: Option<Handle<ImageAsset>>,
    pub normal_texture: Option<Handle<ImageAsset>>,
    pub occlusion_texture: Option<Handle<ImageAsset>>,
    pub emissive_texture: Option<Handle<ImageAsset>>,
}

impl Default for PbrMaterialSource {
    fn default() -> Self {
        PbrMaterialSource {
            name: "<noname>".to_string(),
            base_color_factor: [1.0, 1.0, 1.0, 1.0],
            emissive_factor: [0.0, 0.0, 0.0],
            metallic_factor: 1.0,
            roughness_factor: 1.0,
            normal_texture_scale: 1.0,
            occlusion_texture_strength: 1.0,
            alpha_cutoff: 0.5,
            base_color_texture: None,
            metallic_roughness_texture: None,
            normal_texture: None,
            occlusion_texture: None,
            emissive_texture: None,
        }
    }
}

impl Into<MeshMaterialDataShaderParam> for PbrMaterialSource {
    fn into(self) -> MeshMaterialDataShaderParam {
        MeshMaterialDataShaderParam {
            base_color_factor: self.base_color_factor.into(),
            emissive_factor: self.emissive_factor.into(),
            metallic_factor: self.metallic_factor,
            roughness_factor: self.roughness_factor,
            normal_texture_scale: self.normal_texture_scale,
            occlusion_texture_strength: self.occlusion_texture_strength,
            alpha_cutoff: self.alpha_cutoff,
            has_base_color_texture: self.base_color_texture.is_some() as u32,
            has_metallic_roughness_texture: self.metallic_roughness_texture.is_some() as u32,
            has_normal_texture: self.normal_texture.is_some() as u32,
            has_occlusion_texture: self.occlusion_texture.is_some() as u32,
            has_emissive_texture: self.emissive_texture.is_some() as u32,
            ..Default::default()
        }
    }
}

#[derive(TypeUuid, Serialize, Deserialize, Debug, Clone)]
#[uuid = "54670c73-8b04-4fa3-acf3-ec63deff0a99"]
pub struct PbrMaterialAssetData {
    pub source: PbrMaterialSource,
    pub material: Handle<MaterialInstanceAsset>,
}

#[derive(TypeUuid, Clone)]
#[uuid = "4b5d4341-1d48-4051-a283-db545fb4a4f0"]
pub struct PbrMaterialAsset {
    pub inner: Arc<MaterialInstanceAsset>,
}

impl PbrMaterialAsset {
    pub fn get_material_instance(&self) -> MaterialInstanceAsset {
        self.inner.as_ref().clone()
    }
}

pub struct PbrMaterialLoadHandler;

impl DefaultAssetTypeLoadHandler<PbrMaterialAssetData, PbrMaterialAsset>
    for PbrMaterialLoadHandler
{
    #[profiling::function]
    fn load(
        asset_manager: &mut AssetManager,
        asset_data: PbrMaterialAssetData,
    ) -> RafxResult<PbrMaterialAsset> {
        if let Some(material) = asset_manager.committed_asset(&asset_data.material) {
            Ok(PbrMaterialAsset {
                inner: Arc::new(material.clone()),
            })
        } else {
            Err(RafxError::StringError(format!(
                "PbrMaterial {} failed loading material instance asset",
                asset_data.source.name
            )))
        }
    }
}

pub type PbrMaterialAssetType =
    DefaultAssetTypeHandler<PbrMaterialAssetData, PbrMaterialAsset, PbrMaterialLoadHandler>;
