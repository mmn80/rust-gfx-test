use super::SceneManagerAction;
#[cfg(feature = "use-imgui")]
use crate::features::imgui::ImGuiRenderFeature;
use crate::scenes::Scene;
use crate::{input::InputState, phases::UiRenderPhase};
use legion::{Resources, World};
use rafx::rafx_visibility::{DepthRange, OrthographicParameters, Projection};
use rafx::render_features::{
    RenderFeatureMaskBuilder, RenderPhaseMaskBuilder, RenderViewDepthRange,
};
use rafx::renderer::RenderViewMeta;
use rafx::visibility::{ViewFrustumArc, VisibilityRegion};
use rafx::{api::RafxSwapchainHelper, renderer::ViewportsResource};
use winit::event::VirtualKeyCode;

pub(super) struct MenuScene {
    main_view_frustum: ViewFrustumArc,
}

impl MenuScene {
    pub(super) fn new(_world: &mut World, resources: &Resources) -> Self {
        let mut viewports_resource = resources.get_mut::<ViewportsResource>().unwrap();
        let visibility_region = resources.get::<VisibilityRegion>().unwrap();

        let main_camera_phase_mask = RenderPhaseMaskBuilder::default()
            .add_render_phase::<UiRenderPhase>()
            .build();

        #[cfg(feature = "use-imgui")]
        let main_camera_feature_mask = RenderFeatureMaskBuilder::default()
            .add_render_feature::<ImGuiRenderFeature>()
            .build();
        #[cfg(not(feature = "use-imgui"))]
        let main_camera_feature_mask = RenderFeatureMaskBuilder::default().build();

        let eye = glam::Vec3::new(1400.0, -200.0, 1000.0);

        let half_width = viewports_resource.main_window_size.width as f32 / 2.0;
        let half_height = viewports_resource.main_window_size.height as f32 / 2.0;

        let look_at = eye.truncate().extend(0.0);
        let up = glam::Vec3::new(0.0, 1.0, 0.0);

        let view = glam::Mat4::look_at_rh(eye, look_at, up);

        let near = 0.01;
        let far = 2000.0;

        let projection = Projection::Orthographic(OrthographicParameters::new(
            -half_width,
            half_width,
            -half_height,
            half_height,
            near,
            far,
            DepthRange::InfiniteReverse,
        ));

        let main_view_frustum = visibility_region.register_view_frustum();
        main_view_frustum
            .set_projection(&projection)
            .set_transform(eye, look_at, up);

        viewports_resource.main_view_meta = Some(RenderViewMeta {
            view_frustum: main_view_frustum.clone(),
            eye_position: eye,
            view,
            proj: projection.as_rh_mat4(),
            depth_range: RenderViewDepthRange::from_projection(&projection),
            render_phase_mask: main_camera_phase_mask,
            render_feature_mask: main_camera_feature_mask,
            debug_name: "main".to_string(),
        });

        MenuScene { main_view_frustum }
    }
}

impl super::GameScene for MenuScene {
    fn update(&mut self, _world: &mut World, resources: &mut Resources) -> SceneManagerAction {
        let mut action = SceneManagerAction::None;
        #[cfg(feature = "use-imgui")]
        {
            use crate::features::imgui::ImguiManager;
            profiling::scope!("imgui");
            let imgui_manager = resources.get::<ImguiManager>().unwrap();
            let swapchain_helper = resources.get::<RafxSwapchainHelper>().unwrap();
            imgui_manager.with_ui(|ui| {
                profiling::scope!("main game menu");

                let menu_window = imgui::Window::new(imgui::im_str!("Home"));
                menu_window
                    .position(
                        [
                            (swapchain_helper.swapchain_def().width as f32) / 2.0,
                            (swapchain_helper.swapchain_def().height as f32) / 2.0,
                        ],
                        imgui::Condition::Always,
                    )
                    .position_pivot([0.5, 0.5])
                    .title_bar(false)
                    .always_auto_resize(true)
                    .resizable(false)
                    .movable(false)
                    .scroll_bar(false)
                    .collapsible(false)
                    .build(&ui, || {
                        if ui.button(imgui::im_str!("Play"), [200.0_f32, 100.0]) {
                            action = SceneManagerAction::Scene(Scene::Main);
                        }
                        if ui.button(imgui::im_str!("Exit"), [200.0_f32, 100.0]) {
                            action = SceneManagerAction::Exit;
                        }
                    });
            });
        }

        let input = resources.get::<InputState>().unwrap();

        if input.key_trigger.contains(&VirtualKeyCode::Escape) {
            action = SceneManagerAction::Exit;
        }
        if input.key_trigger.contains(&VirtualKeyCode::S) {
            action = SceneManagerAction::Scene(Scene::Main);
        }

        action
    }
}
