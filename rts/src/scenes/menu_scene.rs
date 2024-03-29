use egui::{Align2, Button};
use legion::Resources;
use rafx::{
    rafx_visibility::{DepthRange, OrthographicParameters, Projection},
    render_features::{
        RenderFeatureFlagMaskBuilder, RenderFeatureMaskBuilder, RenderPhaseMaskBuilder,
        RenderViewDepthRange,
    },
    renderer::{RenderViewMeta, ViewportsResource},
};
use rafx_plugins::{
    features::egui::{EguiContextResource, EguiRenderFeature},
    phases::UiRenderPhase,
};

use super::SceneManagerAction;
use crate::{
    camera::RTSCamera,
    env::simulation::Simulation,
    input::{InputResource, KeyboardKey},
    scenes::Scene,
    ui::UiState,
};

pub(super) struct MenuScene {}

impl MenuScene {
    pub(super) fn new(simulation: &mut Simulation, resources: &Resources) -> Self {
        let mut viewports_resource = resources.get_mut::<ViewportsResource>().unwrap();
        let camera = resources.get::<RTSCamera>().unwrap();

        let eye = glam::Vec3::new(1400.0, -200.0, 1000.0);
        let half_width = camera.win_width as f32 / 2.0;
        let half_height = camera.win_height as f32 / 2.0;
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

        simulation
            .universe()
            .main_view_frustum
            .set_projection(&projection)
            .set_transform(eye, look_at, up);

        let render_phase_mask = RenderPhaseMaskBuilder::default()
            .add_render_phase::<UiRenderPhase>()
            .build();
        let render_feature_mask = RenderFeatureMaskBuilder::default()
            .add_render_feature::<EguiRenderFeature>()
            .build();
        let render_feature_flag_mask = RenderFeatureFlagMaskBuilder::default().build();

        viewports_resource.main_view_meta = Some(RenderViewMeta {
            view_frustum: simulation.universe().main_view_frustum.clone(),
            eye_position: eye,
            view,
            proj: projection.as_rh_mat4(),
            depth_range: RenderViewDepthRange::from_projection(&projection),
            render_phase_mask,
            render_feature_mask,
            render_feature_flag_mask,
            debug_name: "main".to_string(),
        });

        MenuScene {}
    }
}

impl super::GameScene for MenuScene {
    fn update(
        &mut self,
        simulation: &mut Simulation,
        resources: &mut Resources,
        ui_state: &mut UiState,
    ) -> SceneManagerAction {
        ui_state.update(simulation, resources, None, None, None);

        let mut action = SceneManagerAction::None;

        let context = resources.get::<EguiContextResource>().unwrap().context();
        let scale_factor = context.pixels_per_point();

        profiling::scope!("egui");
        egui::Area::new("Home")
            .anchor(Align2::CENTER_CENTER, [0., 0.])
            .movable(false)
            .show(&context, |ui| {
                let btn_size = [150.0 / scale_factor, 75.0 / scale_factor];
                if ui.add_sized(btn_size, Button::new("PLAY")).clicked() {
                    action = SceneManagerAction::Scene(Scene::Main);
                }
                if ui.add_sized(btn_size, Button::new("EXIT")).clicked() {
                    action = SceneManagerAction::Exit;
                }
            });

        let input = resources.get::<InputResource>().unwrap();
        if input.is_key_just_up(KeyboardKey::Escape) {
            action = SceneManagerAction::Exit;
        }
        if input.is_key_just_up(KeyboardKey::S) {
            action = SceneManagerAction::Scene(Scene::Main);
        }

        action
    }
}
