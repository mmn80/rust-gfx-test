use crate::{
    assets::{font::FontAssetTypeRendererPlugin, gltf::GltfAssetTypeRendererPlugin},
    camera::RTSCamera,
    features::{
        debug3d::Debug3DRendererPlugin, egui::EguiRendererPlugin, mesh::MeshRendererPlugin,
        text::TextRendererPlugin,
    },
    render_graph_generator::DemoRenderGraphGenerator,
    DemoRendererPlugin,
};
use legion::Resources;
use rafx::{
    api::{RafxApi, RafxDeviceContext, RafxResult, RafxSwapchainHelper},
    assets::{distill_impl::AssetResource, AssetManager},
    framework::visibility::VisibilityRegion,
    render_features::{ExtractResources, RenderRegistry},
    renderer::{
        AssetSource, Renderer, RendererBuilder, RendererConfigResource, SwapchainHandler,
        ViewportsResource,
    },
};
use std::sync::Arc;
use winit::{event_loop::EventLoop, window::Window};

pub fn window_init(event_loop: &EventLoop<()>) -> Window {
    // Set up the coordinate system to be fixed at 900x600, and use this as the default window size
    // This means the drawing code can be written as though the window is always 900x600. The
    // output will be automatically scaled so that it's always visible.
    let logical_size = winit::dpi::LogicalSize::new(900.0, 600.0);

    // Create a single window
    winit::window::WindowBuilder::new()
        .with_title("RTS MMO")
        .with_inner_size(logical_size)
        //.with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)))
        .build(event_loop)
        .expect("Failed to create window")
}

pub fn rendering_init(
    resources: &mut Resources,
    window: &Window,
    asset_source: AssetSource,
) -> RafxResult<()> {
    resources.insert(VisibilityRegion::new());
    resources.insert(ViewportsResource::default());
    resources.insert(RTSCamera::default());

    let mesh_renderer_plugin = Arc::new(MeshRendererPlugin::new(Some(32)));
    let debug3d_renderer_plugin = Arc::new(Debug3DRendererPlugin::default());
    let text_renderer_plugin = Arc::new(TextRendererPlugin::default());
    let egui_renderer_plugin = Arc::new(EguiRendererPlugin::default());
    mesh_renderer_plugin.legion_init(resources);
    debug3d_renderer_plugin.legion_init(resources);
    text_renderer_plugin.legion_init(resources);
    egui_renderer_plugin.legion_init(resources, &window);

    //
    // Create the api. GPU programming is fundamentally unsafe, so all rafx APIs should be
    // considered unsafe. However, rafx APIs are only gated by unsafe if they can cause undefined
    // behavior on the CPU for reasons other than interacting with the GPU.
    //
    let rafx_api = unsafe { rafx::api::RafxApi::new(window, &Default::default())? };

    let mut renderer_builder = RendererBuilder::default();
    renderer_builder = renderer_builder
        .add_asset(Arc::new(FontAssetTypeRendererPlugin))
        .add_asset(Arc::new(GltfAssetTypeRendererPlugin))
        .add_asset(Arc::new(DemoRendererPlugin))
        .add_render_feature(mesh_renderer_plugin)
        .add_render_feature(debug3d_renderer_plugin)
        .add_render_feature(text_renderer_plugin)
        .add_render_feature(egui_renderer_plugin);

    let mut renderer_builder_result = {
        let extract_resources = ExtractResources::default();

        let render_graph_generator = Box::new(DemoRenderGraphGenerator);

        renderer_builder.build(
            extract_resources,
            &rafx_api,
            asset_source,
            render_graph_generator,
            None, // Some(Box::new(DemoRendererThreadPool::new())),
        )
    }?;

    let size = window.inner_size();
    let swapchain_helper = SwapchainHandler::create_swapchain(
        &mut renderer_builder_result.asset_manager,
        &mut renderer_builder_result.renderer,
        window,
        size.width,
        size.height,
    )?;

    resources.insert(rafx_api.device_context());
    resources.insert(rafx_api);
    resources.insert(swapchain_helper);
    resources.insert(renderer_builder_result.asset_resource);
    resources.insert(
        renderer_builder_result
            .asset_manager
            .resource_manager()
            .render_registry()
            .clone(),
    );
    resources.insert(renderer_builder_result.asset_manager);
    resources.insert(renderer_builder_result.renderer);
    resources.insert(RendererConfigResource::default());

    Ok(())
}

pub fn rendering_destroy(resources: &mut Resources) -> RafxResult<()> {
    // Destroy these first
    {
        {
            let swapchain_helper = resources.remove::<RafxSwapchainHelper>().unwrap();
            let mut asset_manager = resources.get_mut::<AssetManager>().unwrap();
            let renderer = resources.get::<Renderer>().unwrap();
            SwapchainHandler::destroy_swapchain(swapchain_helper, &mut *asset_manager, &*renderer)?;
        }

        resources.remove::<Renderer>();

        #[cfg(feature = "use-imgui")]
        {
            use crate::features::imgui::ImGuiRendererPlugin;
            ImGuiRendererPlugin::legion_destroy(resources);
        }

        MeshRendererPlugin::legion_destroy(resources);
        Debug3DRendererPlugin::legion_destroy(resources);
        TextRendererPlugin::legion_destroy(resources);
        EguiRendererPlugin::legion_destroy(resources);

        resources.remove::<RenderRegistry>();

        // Remove the asset resource because we have asset storages that reference resources
        resources.remove::<AssetResource>();

        resources.remove::<AssetManager>();
        resources.remove::<RafxDeviceContext>();
    }

    // Drop this one last
    resources.remove::<RafxApi>();
    Ok(())
}
