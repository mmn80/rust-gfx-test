use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use glam::{Mat4, Quat, Vec3, Vec4Swizzles};
use rafx::{
    rafx_visibility::{DepthRange, PerspectiveParameters, Projection},
    render_features::{
        RenderFeatureFlagMaskBuilder, RenderFeatureMaskBuilder, RenderPhaseMaskBuilder,
        RenderViewDepthRange,
    },
    renderer::{RenderViewMeta, ViewportsResource},
    visibility::ViewFrustumArc,
};
use rafx_plugins::{
    features::{
        debug3d::Debug3DRenderFeature,
        debug_pip::DebugPipRenderFeature,
        egui::EguiRenderFeature,
        mesh_adv::{
            MeshAdvNoShadowsRenderFeatureFlag as MeshNoShadowsRenderFeatureFlag,
            MeshAdvUnlitRenderFeatureFlag as MeshUnlitRenderFeatureFlag,
            MeshAdvUntexturedRenderFeatureFlag as MeshUntexturedRenderFeatureFlag,
            MeshAdvWireframeRenderFeatureFlag as MeshWireframeRenderFeatureFlag,
        },
        text::TextRenderFeature,
    },
    phases::{
        DebugPipRenderPhase, DepthPrepassRenderPhase, OpaqueRenderPhase, TransparentRenderPhase,
        UiRenderPhase, WireframeRenderPhase,
    },
};

use crate::{
    env::simulation::{RayCastResult, Universe},
    features::dyn_mesh::{
        DynMeshNoShadowsRenderFeatureFlag, DynMeshRenderFeature, DynMeshUnlitRenderFeatureFlag,
        DynMeshUntexturedRenderFeatureFlag, DynMeshWireframeRenderFeatureFlag,
    },
    input::{InputResource, KeyboardKey},
    time::TimeState,
    ui::UiState,
    RenderOptions,
};

#[derive(Clone, Copy)]
pub struct RTSCamera {
    pub pitch_default: f32,
    pub pitch_zero_height: f32,
    pub pitch_height_power: i32,
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
    pub win_width: u32,
    pub win_height: u32,
    pub win_scale_factor: f32,
}

impl Default for RTSCamera {
    fn default() -> Self {
        Self {
            pitch_default: 45.,
            pitch_zero_height: 100.,
            pitch_height_power: 2,
            look_at: Vec3::new(0., 0., 0.),
            look_at_dist: 40.,
            yaw: 0.,
            pitch: FRAC_PI_4,
            move_speed: 20.,
            yaw_speed: 5.,
            scroll_speed: 50.,
            fov_y: std::f32::consts::FRAC_PI_4,
            near_plane: 0.01,
            far_plane: 10000.,
            view_matrix: glam::Mat4::IDENTITY,
            projection_matrix: glam::Mat4::IDENTITY,
            win_width: 0,
            win_height: 0,
            win_scale_factor: 1.,
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

    fn pitch_by_distance(&self) -> f32 {
        (1.0 - (self.look_at_dist / self.pitch_zero_height).powi(self.pitch_height_power))
            .min(1.0)
            .max(0.0)
            * self.pitch_default
            * PI
            / 180.
    }

    pub fn make_ray(&self, screen_x: u32, screen_y: u32) -> Vec3 {
        // https://antongerdelan.net/opengl/raycasting.html
        let ray_nds = Vec3::new(
            (2. * screen_x as f32) / self.win_width as f32 - 1.,
            1. - (2. * screen_y as f32) / self.win_height as f32,
            1.,
        );
        let ray_clip = glam::Vec4::new(ray_nds.x, ray_nds.y, -1.0, 1.0);
        let ray_eye = self.projection_matrix.inverse() * ray_clip;
        let ray_eye = glam::Vec4::new(ray_eye.x, ray_eye.y, -1.0, 0.0);
        (self.view_matrix.inverse() * ray_eye).xyz().normalize()
    }

    pub fn ray_cast_terrain(
        &self,
        screen_x: u32,
        screen_y: u32,
        universe: &Universe,
        ui_state: &mut UiState,
    ) -> Option<RayCastResult> {
        let eye = self.eye();
        let ray = self.make_ray(screen_x, screen_y);
        if let Some(result) = universe.ray_cast(eye, ray) {
            Some(result)
        } else {
            ui_state.error(format!(
                "Failed terrain ray cast, start: {}, ray: {}",
                eye, ray
            ));
            None
        }
    }

    pub fn ray_cast_screen(&self, screen_x: u32, screen_y: u32, screen_center_ray: Vec3) -> Vec3 {
        let ray_vec = self.make_ray(screen_x, screen_y);
        let angle = ray_vec.angle_between(screen_center_ray);
        let len = (self.near_plane + 1.) / f32::cos(angle);
        self.eye() + len * ray_vec
    }

    fn update_transform(&mut self, dt: f32, input: &InputResource) {
        if input.is_key_down(KeyboardKey::W) {
            self.look_at += dt * self.move_speed * self.forward();
        }
        if input.is_key_down(KeyboardKey::S) {
            self.look_at -= dt * self.move_speed * self.forward();
        }
        if input.is_key_down(KeyboardKey::A) {
            self.look_at += dt * self.move_speed * self.right();
        }
        if input.is_key_down(KeyboardKey::D) {
            self.look_at -= dt * self.move_speed * self.right();
        }
        if input.is_key_down(KeyboardKey::Q) {
            self.yaw -= dt * self.yaw_speed;
        }
        if input.is_key_down(KeyboardKey::E) {
            self.yaw += dt * self.yaw_speed;
        }
        if input.mouse_wheel_delta().y.abs() > f32::EPSILON {
            self.look_at_dist = (self.look_at_dist
                + self.scroll_speed
                    * input.mouse_wheel_delta().y
                    * dt
                    * (self.look_at_dist / 10.0))
                .max(1.)
                .min(1000.);
            self.pitch = self.pitch_by_distance();
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
        let phase_mask_builder = RenderPhaseMaskBuilder::default()
            .add_render_phase::<DepthPrepassRenderPhase>()
            .add_render_phase::<OpaqueRenderPhase>()
            .add_render_phase::<TransparentRenderPhase>()
            .add_render_phase::<WireframeRenderPhase>()
            .add_render_phase::<DebugPipRenderPhase>()
            .add_render_phase::<UiRenderPhase>();

        let mut feature_mask_builder = RenderFeatureMaskBuilder::default()
            .add_render_feature::<MeshBasicRenderFeature>()
            .add_render_feature::<DynMeshRenderFeature>()
            .add_render_feature::<EguiRenderFeature>()
            .add_render_feature::<DebugPipRenderFeature>();

        if render_options.show_text {
            feature_mask_builder = feature_mask_builder.add_render_feature::<TextRenderFeature>();
        }

        if render_options.show_debug3d {
            feature_mask_builder =
                feature_mask_builder.add_render_feature::<Debug3DRenderFeature>();
        }

        let mut feature_flag_mask_builder = RenderFeatureFlagMaskBuilder::default();

        if render_options.show_wireframes {
            feature_flag_mask_builder = feature_flag_mask_builder
                .add_render_feature_flag::<MeshWireframeRenderFeatureFlag>();
            feature_flag_mask_builder = feature_flag_mask_builder
                .add_render_feature_flag::<DynMeshWireframeRenderFeatureFlag>();
        }

        if !render_options.enable_lighting {
            feature_flag_mask_builder =
                feature_flag_mask_builder.add_render_feature_flag::<MeshUnlitRenderFeatureFlag>();
            feature_flag_mask_builder = feature_flag_mask_builder
                .add_render_feature_flag::<DynMeshUnlitRenderFeatureFlag>();
        }

        if !render_options.enable_textures {
            feature_flag_mask_builder = feature_flag_mask_builder
                .add_render_feature_flag::<MeshUntexturedRenderFeatureFlag>();
            feature_flag_mask_builder = feature_flag_mask_builder
                .add_render_feature_flag::<DynMeshUntexturedRenderFeatureFlag>();
        }

        if !render_options.show_shadows {
            feature_flag_mask_builder = feature_flag_mask_builder
                .add_render_feature_flag::<MeshNoShadowsRenderFeatureFlag>();
            feature_flag_mask_builder = feature_flag_mask_builder
                .add_render_feature_flag::<DynMeshNoShadowsRenderFeatureFlag>();
        }

        viewports_resource.main_view_meta = Some(RenderViewMeta {
            view_frustum: main_view_frustum.clone(),
            eye_position: eye,
            view: self.view_matrix,
            proj: self.projection_matrix,
            depth_range: RenderViewDepthRange::from_projection(projection),
            render_phase_mask: phase_mask_builder.build(),
            render_feature_mask: feature_mask_builder.build(),
            render_feature_flag_mask: feature_flag_mask_builder.build(),
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
        input: &InputResource,
    ) {
        self.update_transform(time_state.previous_update_dt(), input);

        let aspect_ratio = self.win_width as f32 / self.win_height.max(1) as f32;

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

    pub fn update_ui(&mut self, _ui_state: &mut UiState, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("RTS Camera")
            .default_open(false)
            .show(ui, |ui| {
                let old_pitch_default = self.pitch_default;
                let old_pitch_zero_height = self.pitch_zero_height;
                let old_pitch_height_power = self.pitch_height_power;
                ui.add(egui::Slider::new(&mut self.pitch_default, 0.0..=90.).text("default pitch"));
                ui.add(
                    egui::Slider::new(&mut self.pitch_zero_height, 10.0..=500.).text("pitch max h"),
                );
                ui.add(
                    egui::Slider::new(&mut self.pitch_height_power, 1..=8).text("pitch h power"),
                );
                if old_pitch_default != self.pitch_default
                    || old_pitch_zero_height != self.pitch_zero_height
                    || old_pitch_height_power != self.pitch_height_power
                {
                    self.pitch = self.pitch_by_distance();
                }
            });
    }
}
