use crate::assets::pbr_material::{PbrMaterialAssetData, PbrMaterialSource};
use distill::{
    core::AssetUuid,
    importer::{ImportOp, ImportedAsset, Importer, ImporterValue},
    loader::handle::Handle,
    make_handle, make_handle_from_str,
};
use rafx::assets::{ImageAsset, MaterialInstanceAssetData, MaterialInstanceSlotAssignment};
use rafx_plugins::assets::mesh::MeshMaterialDataShaderParam;
use serde::{Deserialize, Serialize};
use std::io::Read;
use type_uuid::*;

#[derive(TypeUuid, Serialize, Deserialize, Default, Clone, Debug)]
#[uuid = "6b5e8cc4-9a8e-45e2-9e25-7f1ab02f4ca0"]
pub struct PbrMaterialImporterStateStable {
    terrain_asset_uuid: Option<AssetUuid>,
    material_instance_asset_uuid: Option<AssetUuid>,
}

impl From<PbrMaterialImporterStateUnstable> for PbrMaterialImporterStateStable {
    fn from(other: PbrMaterialImporterStateUnstable) -> Self {
        let mut stable = PbrMaterialImporterStateStable::default();
        stable.terrain_asset_uuid = other.terrain_asset_uuid.clone();
        stable.material_instance_asset_uuid = other.material_instance_asset_uuid.clone();
        stable
    }
}

#[derive(Default)]
pub struct PbrMaterialImporterStateUnstable {
    terrain_asset_uuid: Option<AssetUuid>,
    material_instance_asset_uuid: Option<AssetUuid>,
}

impl From<PbrMaterialImporterStateStable> for PbrMaterialImporterStateUnstable {
    fn from(other: PbrMaterialImporterStateStable) -> Self {
        let mut unstable = PbrMaterialImporterStateUnstable::default();
        unstable.terrain_asset_uuid = other.terrain_asset_uuid.clone();
        unstable.material_instance_asset_uuid = other.material_instance_asset_uuid.clone();
        unstable
    }
}

#[derive(TypeUuid)]
#[uuid = "32ca7189-ac8b-4e4e-a7a3-4f43e115bc1f"]
pub struct PbrMaterialImporter;
impl Importer for PbrMaterialImporter {
    fn version_static() -> u32
    where
        Self: Sized,
    {
        1
    }

    fn version(&self) -> u32 {
        Self::version_static()
    }

    type Options = ();
    type State = PbrMaterialImporterStateStable;

    #[profiling::function]
    fn import(
        &self,
        op: &mut ImportOp,
        source: &mut dyn Read,
        _options: &Self::Options,
        stable_state: &mut Self::State,
    ) -> distill::importer::Result<ImporterValue> {
        let mut imported_assets = Vec::<ImportedAsset>::default();

        let mut unstable_state: PbrMaterialImporterStateUnstable = stable_state.clone().into();
        unstable_state.terrain_asset_uuid = Some(
            unstable_state
                .terrain_asset_uuid
                .unwrap_or_else(|| AssetUuid(*uuid::Uuid::new_v4().as_bytes())),
        );

        let source = ron::de::from_reader::<_, PbrMaterialSource>(source)?;
        let material_handle = make_handle_from_str("92a98639-de0d-40cf-a222-354f616346c3")?;
        let null_image_handle = make_handle_from_str("fc937369-cad2-4a00-bf42-5968f1210784")?;

        let material_instance_uuid = if let Some(uuid) = unstable_state.material_instance_asset_uuid
        {
            uuid
        } else {
            let material_instance_uuid = op.new_asset_uuid();
            unstable_state.material_instance_asset_uuid = Some(material_instance_uuid);
            material_instance_uuid
        };

        let material_instance_handle = make_handle(material_instance_uuid);

        let mut search_tags: Vec<(String, Option<String>)> = vec![];
        search_tags.push(("name".to_string(), Some(source.name.clone())));

        let mut slot_assignments = vec![];

        let material_data_shader_param: MeshMaterialDataShaderParam = source.clone().into();
        slot_assignments.push(MaterialInstanceSlotAssignment {
            slot_name: "per_material_data".to_string(),
            array_index: 0,
            image: None,
            sampler: None,
            buffer_data: Some(rafx::base::memory::any_as_bytes(&material_data_shader_param).into()),
        });

        fn push_image_slot_assignment(
            slot_name: &str,
            slot_assignments: &mut Vec<MaterialInstanceSlotAssignment>,
            image: &Option<Handle<ImageAsset>>,
            default_image: &Handle<ImageAsset>,
        ) {
            slot_assignments.push(MaterialInstanceSlotAssignment {
                slot_name: slot_name.to_string(),
                array_index: 0,
                image: if image.is_some() {
                    Some(image.as_ref().map_or(default_image, |x| x).clone())
                } else {
                    Some(default_image.clone())
                },
                sampler: None,
                buffer_data: None,
            });
        }

        push_image_slot_assignment(
            "base_color_texture",
            &mut slot_assignments,
            &source.base_color_texture,
            &null_image_handle,
        );
        push_image_slot_assignment(
            "metallic_roughness_texture",
            &mut slot_assignments,
            &source.metallic_roughness_texture,
            &null_image_handle,
        );
        push_image_slot_assignment(
            "normal_texture",
            &mut slot_assignments,
            &source.normal_texture,
            &null_image_handle,
        );
        push_image_slot_assignment(
            "occlusion_texture",
            &mut slot_assignments,
            &source.occlusion_texture,
            &null_image_handle,
        );
        push_image_slot_assignment(
            "emissive_texture",
            &mut slot_assignments,
            &source.emissive_texture,
            &null_image_handle,
        );

        let material_instance_asset = MaterialInstanceAssetData {
            material: material_handle.clone(),
            slot_assignments,
        };

        log::debug!(
            "Importing material instance uuid {:?}",
            material_instance_uuid
        );

        imported_assets.push(ImportedAsset {
            id: material_instance_uuid,
            search_tags,
            build_deps: vec![],
            load_deps: vec![],
            build_pipeline: None,
            asset_data: Box::new(material_instance_asset),
        });

        imported_assets.push(ImportedAsset {
            id: unstable_state.terrain_asset_uuid.unwrap(),
            search_tags: vec![],
            build_deps: vec![],
            load_deps: vec![],
            build_pipeline: None,
            asset_data: Box::new(PbrMaterialAssetData {
                source,
                material: material_instance_handle,
            }),
        });

        *stable_state = unstable_state.into();

        Ok(ImporterValue {
            assets: imported_assets,
        })
    }
}
