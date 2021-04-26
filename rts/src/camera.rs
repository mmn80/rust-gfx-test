use crate::features::debug3d::Debug3DRenderFeature;
#[cfg(feature = "use-imgui")]
use crate::features::imgui::ImGuiRenderFeature;
use crate::features::mesh::MeshRenderFeature;
use crate::features::text::TextRenderFeature;
use crate::phases::{
    DepthPrepassRenderPhase, OpaqueRenderPhase, TransparentRenderPhase, UiRenderPhase,
};
use crate::{time::TimeState, RenderOptions};
use glam::{Quat, Vec3};
use rafx::{
    nodes::{RenderFeatureMaskBuilder, RenderPhaseMaskBuilder, RenderViewDepthRange},
    rafx_visibility::{DepthRange, PerspectiveParameters, Projection},
    renderer::{RenderViewMeta, ViewportsResource},
    visibility::ViewFrustumArc,
};
use sdl2::{event::Event, keyboard::Keycode};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

#[derive(Clone, Copy)]
pub struct RTSCamera {
    pub look_at: Vec3,
    pub look_at_dist: f32,
    pub yaw: f32,
    pub pitch: f32,
    move_speed: f32,
    yaw_speed: f32,
    scroll_speed: f32,
}

impl Default for RTSCamera {
    fn default() -> Self {
        Self {
            look_at: Vec3::new(0., 0., 0.),
            look_at_dist: 40.,
            yaw: 0.,
            pitch: RTSCamera::pitch_by_distance(20.),
            move_speed: 20.,
            yaw_speed: 5.,
            scroll_speed: 50.,
        }
    }
}

impl RTSCamera {
    pub fn eye(&self) -> Vec3 {
        if self.pitch.abs() < f32::EPSILON {
            Vec3::new(self.look_at.x, self.look_at.y, self.look_at_dist)
        } else {
            self.look_at - self.right().cross(self.up()) * self.look_at_dist
        }
    }

    pub fn up(&self) -> Vec3 {
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        Vec3::new(cos_pitch * sin_yaw, cos_pitch * cos_yaw, sin_pitch).normalize()
    }

    pub fn forward(&self) -> Vec3 {
        let up = self.up();
        Vec3::new(up.x, up.y, 0.).normalize()
    }

    pub fn right(&self) -> Vec3 {
        Quat::from_rotation_z(FRAC_PI_2).mul_vec3(self.forward())
    }

    fn pitch_by_distance(distance: f32) -> f32 {
        (1.0 - (distance / 100.0).powi(2)).min(1.0).max(0.0) * FRAC_PI_4
    }

    fn update(&mut self, dt: f32, events: &Vec<Event>) {
        for event in events {
            match event {
                Event::KeyDown {
                    keycode: Some(keycode),
                    keymod: _modifiers,
                    ..
                } => {
                    if keycode == &Keycode::W {
                        self.look_at += dt * self.move_speed * self.forward();
                    }
                    if keycode == &Keycode::S {
                        self.look_at -= dt * self.move_speed * self.forward();
                    }
                    if keycode == &Keycode::A {
                        self.look_at += dt * self.move_speed * self.right();
                    }
                    if keycode == &Keycode::D {
                        self.look_at -= dt * self.move_speed * self.right();
                    }
                    if keycode == &Keycode::Q {
                        self.yaw -= dt * self.yaw_speed;
                    }
                    if keycode == &Keycode::E {
                        self.yaw += dt * self.yaw_speed;
                    }
                }
                Event::MouseWheel { y, .. } => {
                    if *y != 0 {
                        let scroll = *y as f32;
                        self.look_at_dist = (self.look_at_dist
                            + self.scroll_speed * scroll * dt * (self.look_at_dist / 10.0))
                            .max(1.)
                            .min(1000.);
                        self.pitch = RTSCamera::pitch_by_distance(self.look_at_dist);
                    }
                }
                _ => {}
            }
        }
    }

    #[profiling::function]
    pub fn update_main_view_3d(
        &mut self,
        time_state: &TimeState,
        render_options: &RenderOptions,
        main_view_frustum: &mut ViewFrustumArc,
        viewports_resource: &mut ViewportsResource,
        events: &Vec<Event>,
    ) {
        let phase_mask = RenderPhaseMaskBuilder::default()
            .add_render_phase::<DepthPrepassRenderPhase>()
            .add_render_phase::<OpaqueRenderPhase>()
            .add_render_phase::<TransparentRenderPhase>()
            .add_render_phase::<UiRenderPhase>()
            .build();

        #[cfg(feature = "use-imgui")]
        let mut feature_mask_builder = RenderFeatureMaskBuilder::default()
            .add_render_feature::<MeshRenderFeature>()
            .add_render_feature::<ImGuiRenderFeature>();
        #[cfg(not(feature = "use-imgui"))]
        let mut feature_mask_builder =
            RenderFeatureMaskBuilder::default().add_render_feature::<MeshRenderFeature>();

        if render_options.show_text {
            feature_mask_builder = feature_mask_builder.add_render_feature::<TextRenderFeature>();
        }

        if render_options.show_debug3d {
            feature_mask_builder =
                feature_mask_builder.add_render_feature::<Debug3DRenderFeature>();
        }

        let main_camera_feature_mask = feature_mask_builder.build();

        self.update(time_state.previous_update_dt(), events);

        let aspect_ratio = viewports_resource.main_window_size.width as f32
            / viewports_resource.main_window_size.height.max(1) as f32;

        let eye = self.eye();
        let look_at = self.look_at;
        let up = self.up();
        let view = glam::Mat4::look_at_rh(eye, look_at, up);

        let fov_y_radians = std::f32::consts::FRAC_PI_4;
        let near_plane = 0.01;

        let projection = Projection::Perspective(PerspectiveParameters::new(
            fov_y_radians,
            aspect_ratio,
            near_plane,
            10000.,
            DepthRange::InfiniteReverse,
        ));

        main_view_frustum
            .set_projection(&projection)
            .set_transform(eye, look_at, up);

        viewports_resource.main_view_meta = Some(RenderViewMeta {
            view_frustum: main_view_frustum.clone(),
            eye_position: eye,
            view,
            proj: projection.as_rh_mat4(),
            depth_range: RenderViewDepthRange::from_projection(&projection),
            render_phase_mask: phase_mask,
            render_feature_mask: main_camera_feature_mask,
            debug_name: "main".to_string(),
        });
    }
}
