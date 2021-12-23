use legion::{Entity, EntityStore, IntoQuery, Read, World};
use rafx::{
    assets::{AssetManagerExtractRef, AssetManagerRenderResource, MaterialAsset},
    base::resource_ref_map::ResourceRefBorrow,
    distill::loader::handle::Handle,
    render_feature_extract_job_predule::*,
};
use rafx_plugins::components::{
    DirectionalLightComponent, PointLightComponent, SpotLightComponent, TransformComponent,
};

#[cfg(feature = "basic-pipeline")]
use rafx_plugins::features::mesh_basic::MeshBasicRenderOptions as MeshRenderOptions;

#[cfg(not(feature = "basic-pipeline"))]
use rafx_plugins::features::mesh_adv::MeshAdvRenderOptions as MeshRenderOptions;

use super::*;

pub struct DynMeshExtractJob<'extract> {
    world: ResourceRefBorrow<'extract, World>,
    mesh_render_options: Option<ResourceRefBorrow<'extract, MeshRenderOptions>>,
    asset_manager: AssetManagerExtractRef,
    depth_material: Handle<MaterialAsset>,
    render_objects: DynMeshRenderObjectSet,
    mesh_manager: DynMeshManagerExtractRef,
}

impl<'extract> DynMeshExtractJob<'extract> {
    pub fn new(
        extract_context: &RenderJobExtractContext<'extract>,
        frame_packet: Box<DynMeshFramePacket>,
        depth_material: Handle<MaterialAsset>,
        render_objects: DynMeshRenderObjectSet,
    ) -> Arc<dyn RenderFeatureExtractJob<'extract> + 'extract> {
        let mesh_manager = unsafe {
            let dyn_mesh_manager = extract_context.extract_resources.fetch::<DynMeshManager>();
            DynMeshManagerExtractRef::new(&dyn_mesh_manager)
        };
        Arc::new(ExtractJob::new(
            Self {
                world: extract_context.extract_resources.fetch::<World>(),
                mesh_render_options: extract_context
                    .extract_resources
                    .try_fetch::<MeshRenderOptions>(),
                asset_manager: extract_context
                    .render_resources
                    .fetch::<AssetManagerRenderResource>()
                    .extract_ref(),
                depth_material,
                render_objects,
                mesh_manager,
            },
            frame_packet,
        ))
    }
}

impl<'extract> ExtractJobEntryPoints<'extract> for DynMeshExtractJob<'extract> {
    fn begin_per_frame_extract(&self, context: &ExtractPerFrameContext<'extract, '_, Self>) {
        context
            .frame_packet()
            .per_frame_data()
            .set(DynMeshPerFrameData {
                depth_material_pass: self
                    .asset_manager
                    .committed_asset(&self.depth_material)
                    .unwrap()
                    .get_single_material_pass()
                    .ok(),
            });
    }

    fn extract_render_object_instance(
        &self,
        job_context: &mut RenderObjectsJobContext<'extract, DynMeshRenderObject>,
        context: &ExtractRenderObjectInstanceContext<'extract, '_, Self>,
    ) {
        let render_object_static_data = job_context
            .render_objects
            .get_id(context.render_object_id());

        context.set_render_object_instance_data({
            self.mesh_manager
                .get_dyn_mesh(&render_object_static_data.mesh)
                .and_then(|dyn_mesh| {
                    let entry = self.world.entry_ref(context.object_id().into()).unwrap();
                    let transform_component = entry.get_component::<TransformComponent>().unwrap();
                    Some(DynMeshRenderObjectInstanceData {
                        dyn_mesh,
                        translation: transform_component.translation,
                        rotation: transform_component.rotation,
                        scale: transform_component.scale,
                    })
                })
        });
    }

    fn end_per_view_extract(&self, context: &ExtractPerViewContext<'extract, '_, Self>) {
        let mut per_view = DynMeshPerViewData::default();
        let is_lit = !context
            .view()
            .feature_flag_is_relevant::<DynMeshUnlitRenderFeatureFlag>();

        if !is_lit {
            context.view_packet().per_view_data().set(per_view);
            return;
        }

        let world = &*self.world;

        let mut query = <(Entity, Read<DirectionalLightComponent>)>::query();
        for light in query.iter(world).map(|(e, l)| ExtractedDirectionalLight {
            object_id: ObjectId::from(*e),
            light: l.clone(),
        }) {
            let next_index = per_view.num_directional_lights;
            per_view.directional_lights[next_index as usize] = Some(light);
            per_view.num_directional_lights += 1;
        }

        let mut query = <(Entity, Read<TransformComponent>, Read<PointLightComponent>)>::query();
        for light in query.iter(world).map(|(e, p, l)| ExtractedPointLight {
            object_id: ObjectId::from(*e),
            light: l.clone(),
            transform: p.clone(),
        }) {
            let next_index = per_view.num_point_lights;
            per_view.point_lights[next_index as usize] = Some(light);
            per_view.num_point_lights += 1;
        }

        let mut query = <(Entity, Read<TransformComponent>, Read<SpotLightComponent>)>::query();
        for light in query.iter(world).map(|(e, p, l)| ExtractedSpotLight {
            object_id: ObjectId::from(*e),
            light: l.clone(),
            transform: p.clone(),
        }) {
            let next_index = per_view.num_spot_lights;
            per_view.spot_lights[next_index as usize] = Some(light);
            per_view.num_spot_lights += 1;
        }

        if let Some(mesh_render_options) = &self.mesh_render_options {
            per_view.ambient_light = mesh_render_options.ambient_light;
        } else {
            per_view.ambient_light = glam::Vec3::ZERO;
        }

        context.view_packet().per_view_data().set(per_view);
    }

    fn feature_debug_constants(&self) -> &'static RenderFeatureDebugConstants {
        super::render_feature_debug_constants()
    }

    fn feature_index(&self) -> RenderFeatureIndex {
        super::render_feature_index()
    }

    fn new_render_object_instance_job_context(
        &'extract self,
    ) -> Option<RenderObjectsJobContext<'extract, DynMeshRenderObject>> {
        Some(RenderObjectsJobContext::new(self.render_objects.read()))
    }

    type RenderObjectInstanceJobContextT = RenderObjectsJobContext<'extract, DynMeshRenderObject>;
    type RenderObjectInstancePerViewJobContextT = DefaultJobContext;

    type FramePacketDataT = DynMeshRenderFeatureTypes;
}
