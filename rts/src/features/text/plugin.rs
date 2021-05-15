use rafx::render_feature_renderer_prelude::*;

use super::*;
use crate::assets::font::FontAsset;
use crate::phases::UiRenderPhase;
use distill::loader::handle::Handle;
use rafx::assets::MaterialAsset;

pub struct TextStaticResources {
    pub text_material: Handle<MaterialAsset>,
    pub default_font: Handle<FontAsset>,
}

#[derive(Default)]
pub struct TextRendererPlugin;

impl TextRendererPlugin {
    pub fn legion_init(&self, resources: &mut legion::Resources) {
        resources.insert(TextResource::new());
    }

    pub fn legion_destroy(resources: &mut legion::Resources) {
        resources.remove::<TextResource>();
    }
}

impl RenderFeaturePlugin for TextRendererPlugin {
    fn feature_debug_constants(&self) -> &'static RenderFeatureDebugConstants {
        super::render_feature_debug_constants()
    }

    fn feature_index(&self) -> RenderFeatureIndex {
        super::render_feature_index()
    }

    fn is_view_relevant(&self, view: &RenderView) -> bool {
        view.phase_is_relevant::<UiRenderPhase>()
    }

    fn requires_visible_render_objects(&self) -> bool {
        false
    }

    fn configure_render_registry(
        &self,
        render_registry: RenderRegistryBuilder,
    ) -> RenderRegistryBuilder {
        render_registry.register_feature::<TextRenderFeature>()
    }

    fn initialize_static_resources(
        &self,
        asset_manager: &mut AssetManager,
        asset_resource: &mut AssetResource,
        _extract_resources: &ExtractResources,
        render_resources: &mut ResourceMap,
        _upload: &mut RafxTransferUpload,
    ) -> RafxResult<()> {
        let text_material =
            asset_resource.load_asset_path::<MaterialAsset, _>("materials/text.material");
        let default_font =
            asset_resource.load_asset_path::<FontAsset, _>("fonts/mplus-1p-regular.ttf");

        asset_manager.wait_for_asset_to_load(&text_material, asset_resource, "text material")?;

        asset_manager.wait_for_asset_to_load(&default_font, asset_resource, "default font")?;

        render_resources.insert(TextStaticResources {
            text_material,
            default_font,
        });

        render_resources.insert(FontAtlasCache::default());

        Ok(())
    }

    fn new_frame_packet(
        &self,
        frame_packet_size: &FramePacketSize,
    ) -> Box<dyn RenderFeatureFramePacket> {
        Box::new(TextFramePacket::new(
            self.feature_index(),
            frame_packet_size,
        ))
    }

    fn new_extract_job<'extract>(
        &self,
        extract_context: &RenderJobExtractContext<'extract>,
        frame_packet: Box<dyn RenderFeatureFramePacket>,
    ) -> Arc<dyn RenderFeatureExtractJob<'extract> + 'extract> {
        let text_material = extract_context
            .render_resources
            .fetch::<TextStaticResources>()
            .text_material
            .clone();

        TextExtractJob::new(extract_context, frame_packet.into_concrete(), text_material)
    }

    fn new_submit_packet(
        &self,
        frame_packet: &Box<dyn RenderFeatureFramePacket>,
    ) -> Box<dyn RenderFeatureSubmitPacket> {
        let frame_packet: &TextFramePacket = frame_packet.as_ref().as_concrete();
        let num_submit_nodes = frame_packet.per_frame_data().get().text_draw_commands.len();

        let mut view_submit_packets = Vec::with_capacity(frame_packet.view_packets().len());
        for view_packet in frame_packet.view_packets() {
            let view_submit_packet = ViewSubmitPacket::from_view_packet::<UiRenderPhase>(
                view_packet,
                Some(num_submit_nodes),
            );
            view_submit_packets.push(view_submit_packet);
        }

        Box::new(TextSubmitPacket::new(
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
        TextPrepareJob::new(
            prepare_context,
            frame_packet.into_concrete(),
            submit_packet.into_concrete(),
        )
    }

    fn new_write_job<'write>(
        &self,
        write_context: &RenderJobWriteContext<'write>,
        frame_packet: Box<dyn RenderFeatureFramePacket>,
        submit_packet: Box<dyn RenderFeatureSubmitPacket>,
    ) -> Arc<dyn RenderFeatureWriteJob<'write> + 'write> {
        TextWriteJob::new(
            write_context,
            frame_packet.into_concrete(),
            submit_packet.into_concrete(),
        )
    }
}
