#[cfg(feature = "use-imgui")]
use crate::features::imgui::ImGuiRenderFeature;
use crate::features::mesh::MeshRenderFeature;
use crate::features::text::TextRenderFeature;
use crate::phases::{
    DepthPrepassRenderPhase, OpaqueRenderPhase, TransparentRenderPhase, UiRenderPhase,
};
use crate::{features::debug3d::Debug3DRenderFeature, input::InputState};
use crate::{time::TimeState, RenderOptions};
use glam::{Mat4, Quat, Vec3, Vec4Swizzles};
use parry3d::{
    bounding_volume::AABB,
    math::{Point, Vector},
    query::{Ray, RayCast},
};
use rafx::{
    nodes::{RenderFeatureMaskBuilder, RenderPhaseMaskBuilder, RenderViewDepthRange},
    rafx_visibility::{DepthRange, PerspectiveParameters, Projection},
    renderer::{RenderViewMeta, ViewportsResource},
    visibility::ViewFrustumArc,
};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
use winit::event::VirtualKeyCode;

#[derive(Clone, Copy)]
pub struct RTSCamera {
    pub look_at: Vec3,
    pub look_at_dist: f32,
    pub yaw: f32,
    pub pitch: f32,
    move_speed: f32,
    yaw_speed: f32,
    scroll_speed: f32,
    fov_y: f32,
    near_plane: f32,
    far_plane: f32,
    view_matrix: glam::Mat4,
    projection_matrix: glam::Mat4,
    screen_width: u32,
    screen_height: u32,
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
            fov_y: std::f32::consts::FRAC_PI_4,
            near_plane: 0.01,
            far_plane: 10000.,
            view_matrix: glam::Mat4::IDENTITY,
            projection_matrix: glam::Mat4::IDENTITY,
            screen_width: 0,
            screen_height: 0,
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

    pub fn view_proj(&self) -> Mat4 {
        self.projection_matrix * self.view_matrix
    }

    fn pitch_by_distance(distance: f32) -> f32 {
        (1.0 - (distance / 100.0).powi(2)).min(1.0).max(0.0) * FRAC_PI_4
    }

    pub fn make_ray(&self, screen_x: u32, screen_y: u32) -> Vec3 {
        // https://antongerdelan.net/opengl/raycasting.html
        let ray_nds = Vec3::new(
            (2. * screen_x as f32) / self.screen_width as f32 - 1.,
            1. - (2. * screen_y as f32) / self.screen_height as f32,
            1.,
        );
        let ray_clip = glam::Vec4::new(ray_nds.x, ray_nds.y, -1.0, 1.0);
        let ray_eye = self.projection_matrix.inverse() * ray_clip;
        let ray_eye = glam::Vec4::new(ray_eye.x, ray_eye.y, -1.0, 0.0);
        (self.view_matrix.inverse() * ray_eye).xyz().normalize()
    }

    pub fn ray_cast_terrain(&self, screen_x: u32, screen_y: u32) -> Vec3 {
        let ray_vec = self.make_ray(screen_x, screen_y);
        let floor = AABB::new(
            Point::new(-1000., -1000., -2.),
            Point::new(1000., 1000., -1.),
        );
        let eye = self.eye();
        let ray = Ray::new(
            Point::new(eye.x, eye.y, eye.z),
            Vector::new(ray_vec.x, ray_vec.y, ray_vec.z),
        );
        if let Some(toi) = floor.cast_local_ray(&ray, 10000., true) {
            eye + ray_vec * toi
        } else {
            Vec3::ONE
        }
    }

    pub fn ray_cast_screen(&self, screen_x: u32, screen_y: u32, screen_center_ray: Vec3) -> Vec3 {
        let ray_vec = self.make_ray(screen_x, screen_y);
        let angle = ray_vec.angle_between(screen_center_ray);
        let len = (self.near_plane + 1.) / f32::cos(angle);
        self.eye() + len * ray_vec
    }

    fn update_transform(&mut self, dt: f32, input: &InputState) {
        if input.key_pressed.contains(&VirtualKeyCode::W) {
            self.look_at += dt * self.move_speed * self.forward();
        }
        if input.key_pressed.contains(&VirtualKeyCode::S) {
            self.look_at -= dt * self.move_speed * self.forward();
        }
        if input.key_pressed.contains(&VirtualKeyCode::A) {
            self.look_at += dt * self.move_speed * self.right();
        }
        if input.key_pressed.contains(&VirtualKeyCode::D) {
            self.look_at -= dt * self.move_speed * self.right();
        }
        if input.key_pressed.contains(&VirtualKeyCode::Q) {
            self.yaw -= dt * self.yaw_speed;
        }
        if input.key_pressed.contains(&VirtualKeyCode::E) {
            self.yaw += dt * self.yaw_speed;
        }
        if input.last_scroll.abs() > f32::EPSILON {
            self.look_at_dist = (self.look_at_dist
                + self.scroll_speed * input.last_scroll * dt * (self.look_at_dist / 10.0))
                .max(1.)
                .min(1000.);
            self.pitch = RTSCamera::pitch_by_distance(self.look_at_dist);
        }
    }

    fn update_main_view_meta(
        &mut self,
        render_options: &RenderOptions,
        main_view_frustum: &mut ViewFrustumArc,
        viewports_resource: &mut ViewportsResource,
        projection: &Projection,
        eye: Vec3,
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

        viewports_resource.main_view_meta = Some(RenderViewMeta {
            view_frustum: main_view_frustum.clone(),
            eye_position: eye,
            view: self.view_matrix,
            proj: self.projection_matrix,
            depth_range: RenderViewDepthRange::from_projection(projection),
            render_phase_mask: phase_mask,
            render_feature_mask: main_camera_feature_mask,
            debug_name: "main".to_string(),
        });
    }

    #[profiling::function]
    pub fn update(
        &mut self,
        time_state: &TimeState,
        render_options: &RenderOptions,
        main_view_frustum: &mut ViewFrustumArc,
        viewports_resource: &mut ViewportsResource,
        input: &InputState,
    ) {
        self.screen_width = viewports_resource.main_window_size.width;
        self.screen_height = viewports_resource.main_window_size.height;

        self.update_transform(time_state.previous_update_dt(), input);

        let aspect_ratio = self.screen_width as f32 / self.screen_height.max(1) as f32;

        let eye = self.eye();
        let look_at = self.look_at;
        let up = self.up();
        self.view_matrix = glam::Mat4::look_at_rh(eye, look_at, up);

        let projection = Projection::Perspective(PerspectiveParameters::new(
            self.fov_y,
            aspect_ratio,
            self.near_plane,
            self.far_plane,
            DepthRange::InfiniteReverse,
        ));

        main_view_frustum
            .set_projection(&projection)
            .set_transform(eye, look_at, up);

        self.projection_matrix = projection.as_rh_mat4();

        self.update_main_view_meta(
            render_options,
            main_view_frustum,
            viewports_resource,
            &projection,
            eye,
        );
    }
}
