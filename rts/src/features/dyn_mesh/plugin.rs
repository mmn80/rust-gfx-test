use distill::loader::handle::Handle;
use rafx::{assets::MaterialAsset, render_feature_renderer_prelude::*};
use rafx_plugins::{
    features::mesh_adv::MeshAdvShadowMapResource as MeshShadowMapResource,
    phases::{
        DepthPrepassRenderPhase, OpaqueRenderPhase, ShadowMapRenderPhase, TransparentRenderPhase,
        WireframeRenderPhase,
    },
};

use super::*;

pub struct DynMeshStaticResources {
    pub depth_material: Handle<MaterialAsset>,
}

pub struct DynMeshRendererPlugin {
    render_objects: DynMeshRenderObjectSet,
    max_num_mesh_parts: Option<usize>,
}

impl DynMeshRendererPlugin {
    pub fn new(max_num_mesh_parts: Option<usize>) -> Self {
        Self {
            max_num_mesh_parts,
            render_objects: DynMeshRenderObjectSet::default(),
        }
    }

    pub fn legion_init(&self, resources: &mut legion::Resources) {
        resources.insert(DynMeshManager::new());
        resources.insert(self.render_objects.clone());
    }

    pub fn legion_destroy(resources: &mut legion::Resources) {
        resources.remove::<DynMeshRenderObjectSet>();
        resources.remove::<DynMeshManager>();
    }
}

impl RenderFeaturePlugin for DynMeshRendererPlugin {
    fn feature_debug_constants(&self) -> &'static RenderFeatureDebugConstants {
        super::render_feature_debug_constants()
    }

    fn feature_index(&self) -> RenderFeatureIndex {
        super::render_feature_index()
    }

    fn is_view_relevant(&self, view: &RenderView) -> bool {
        view.phase_is_relevant::<DepthPrepassRenderPhase>()
            || view.phase_is_relevant::<ShadowMapRenderPhase>()
            || view.phase_is_relevant::<OpaqueRenderPhase>()
            || view.phase_is_relevant::<TransparentRenderPhase>()
            || view.phase_is_relevant::<WireframeRenderPhase>()
    }

    fn requires_visible_render_objects(&self) -> bool {
        true
    }

    fn configure_render_registry(
        &self,
        render_registry: RenderRegistryBuilder,
    ) -> RenderRegistryBuilder {
        render_registry
            .register_feature::<DynMeshRenderFeature>()
            .register_feature_flag::<DynMeshWireframeRenderFeatureFlag>()
            .register_feature_flag::<DynMeshUntexturedRenderFeatureFlag>()
            .register_feature_flag::<DynMeshUnlitRenderFeatureFlag>()
            .register_feature_flag::<DynMeshNoShadowsRenderFeatureFlag>()
    }

    fn initialize_static_resources(
        &self,
        asset_manager: &mut AssetManager,
        asset_resource: &mut AssetResource,
        _extract_resources: &ExtractResources,
        render_resources: &mut ResourceMap,
        _upload: &mut RafxTransferUpload,
    ) -> RafxResult<()> {
        let depth_material = asset_resource
            .load_asset_path::<MaterialAsset, _>("rafx-plugins/materials/depth.material");

        asset_manager.wait_for_asset_to_load(&depth_material, asset_resource, "depth")?;

        render_resources.insert(DynMeshStaticResources { depth_material });

        let mut shadow_map_resource = render_resources.fetch_mut::<MeshShadowMapResource>();
        shadow_map_resource.add_shadow_map_feature::<DynMeshRenderFeature>();

        Ok(())
    }

    fn new_frame_packet(
        &self,
        frame_packet_size: &FramePacketSize,
    ) -> Box<dyn RenderFeatureFramePacket> {
        Box::new(DynMeshFramePacket::new(
            self.feature_index(),
            frame_packet_size,
        ))
    }

    fn new_extract_job<'extract>(
        &self,
        extract_context: &RenderJobExtractContext<'extract>,
        frame_packet: Box<dyn RenderFeatureFramePacket>,
    ) -> Arc<dyn RenderFeatureExtractJob<'extract> + 'extract> {
        let depth_material = extract_context
            .render_resources
            .fetch::<DynMeshStaticResources>()
            .depth_material
            .clone();

        DynMeshExtractJob::new(
            extract_context,
            frame_packet.into_concrete(),
            depth_material,
            self.render_objects.clone(),
        )
    }

    fn new_submit_packet(
        &self,
        frame_packet: &Box<dyn RenderFeatureFramePacket>,
    ) -> Box<dyn RenderFeatureSubmitPacket> {
        let frame_packet: &DynMeshFramePacket = frame_packet.as_ref().as_concrete();

        let mut view_submit_packets = Vec::with_capacity(frame_packet.view_packets().len());
        for view_packet in frame_packet.view_packets() {
            let num_submit_nodes = if let Some(max_num_mesh_parts) = self.max_num_mesh_parts {
                view_packet.num_render_object_instances() * max_num_mesh_parts
            } else {
                // TODO(dvd): Count exact number of submit nodes required.
                todo!()
            };

            let view = view_packet.view();
            let submit_node_blocks = vec![
                SubmitNodeBlock::with_capacity::<OpaqueRenderPhase>(view, num_submit_nodes),
                SubmitNodeBlock::with_capacity::<DepthPrepassRenderPhase>(view, num_submit_nodes),
                SubmitNodeBlock::with_capacity::<ShadowMapRenderPhase>(view, num_submit_nodes),
                SubmitNodeBlock::with_capacity_and_feature_flag::<
                    WireframeRenderPhase,
                    DynMeshWireframeRenderFeatureFlag,
                >(view, num_submit_nodes),
            ];

            view_submit_packets.push(ViewSubmitPacket::new(
                submit_node_blocks,
                &ViewPacketSize::size_of(view_packet),
            ));
        }

        Box::new(DynMeshSubmitPacket::new(
            self.feature_index(),
            frame_packet.render_object_instances().len(),
            view_submit_packets,
        ))
    }

    fn new_prepare_job<'prepare>(
        &self,
        prepare_context: &RenderJobPrepareContext<'prepare>,
        frame_packet: Box<dyn RenderFeatureFramePacket>,
        submit_packet: Box<dyn RenderFeatureSubmitPacket>,
    ) -> Arc<dyn RenderFeaturePrepareJob<'prepare> + 'prepare> {
        DynMeshPrepareJob::new(
            prepare_context,
            frame_packet.into_concrete(),
            submit_packet.into_concrete(),
            self.render_objects.clone(),
        )
    }

    fn new_write_job<'write>(
        &self,
        write_context: &RenderJobWriteContext<'write>,
        frame_packet: Box<dyn RenderFeatureFramePacket>,
        submit_packet: Box<dyn RenderFeatureSubmitPacket>,
    ) -> Arc<dyn RenderFeatureWriteJob<'write> + 'write> {
        DynMeshWriteJob::new(
            write_context,
            frame_packet.into_concrete(),
            submit_packet.into_concrete(),
        )
    }
}
