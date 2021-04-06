use cgmath::*;
use std::f32::consts::FRAC_PI_2;
use std::time::Duration;
use winit::event::*;

use crate::input;

#[rustfmt::skip]
pub const OPENGL_TO_WGPU_MATRIX: cgmath::Matrix4<f32> = cgmath::Matrix4::new(
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 0.5, 0.0,
    0.0, 0.0, 0.5, 1.0,
);

#[derive(Debug)]
pub struct Camera {
    pub position: Point3<f32>,
    yaw: Rad<f32>,
    pitch: Rad<f32>,
    speed: f32,
    scroll_speed: f32,
}

impl Camera {
    pub fn new<V: Into<Point3<f32>>>(
        position: V,
    ) -> Self {
        Self {
            position: position.into(),
            yaw: cgmath::Deg(0.0).into(),
            pitch: cgmath::Deg(-90.0).into(),
            speed: 10.0,
            scroll_speed: 50.0,
        }
    }

    pub fn calc_matrix(&self) -> Matrix4<f32> {
        let (sin_pitch, cos_pitch) = self.pitch.0.sin_cos();
        let (sin_yaw, cos_yaw) = self.yaw.0.sin_cos();

        Matrix4::look_to_rh(
            self.position,
            Vector3::new(
                cos_pitch * cos_yaw,
                sin_pitch,
                cos_pitch * sin_yaw
            ).normalize(),
            Vector3::unit_y(),
        )
    }

    pub fn update(&mut self, dt: Duration, input: &input::InputState) {
        let dt = dt.as_secs_f32();

        if input.key_pressed.contains(&VirtualKeyCode::W) {
            self.position.x += self.speed * dt;
        }
        if input.key_pressed.contains(&VirtualKeyCode::S) {
            self.position.x -= self.speed * dt;
        }
        if input.key_pressed.contains(&VirtualKeyCode::A) {
            self.position.z -= self.speed * dt;
        }
        if input.key_pressed.contains(&VirtualKeyCode::D) {
            self.position.z += self.speed * dt;
        }
        if input.last_scroll != 0.0 {
            self.position.y += self.scroll_speed * input.last_scroll * dt;
        }
        if input.key_pressed.contains(&VirtualKeyCode::Up) {
            self.pitch += cgmath::Rad(dt);
        }
        if input.key_pressed.contains(&VirtualKeyCode::Down) {
            self.pitch -= cgmath::Rad(dt);
        }
        if input.key_pressed.contains(&VirtualKeyCode::Left) {
            self.yaw += cgmath::Rad(dt);
        }
        if input.key_pressed.contains(&VirtualKeyCode::Right) {
            self.yaw -= cgmath::Rad(dt);
        }

        if self.pitch < -Rad(SAFE_FRAC_PI_2) {
            self.pitch = -Rad(SAFE_FRAC_PI_2);
        } else if self.pitch > Rad(SAFE_FRAC_PI_2) {
            self.pitch = Rad(SAFE_FRAC_PI_2);
        }
    }
}

const SAFE_FRAC_PI_2: f32 = FRAC_PI_2 - 0.0001;

pub struct Projection {
    aspect: f32,
    fovy: Rad<f32>,
    znear: f32,
    zfar: f32,
}

impl Projection {
    pub fn new<F: Into<Rad<f32>>>(width: u32, height: u32, fovy: F, znear: f32, zfar: f32) -> Self {
        Self {
            aspect: width as f32 / height as f32,
            fovy: fovy.into(),
            znear,
            zfar,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.aspect = width as f32 / height as f32;
    }

    pub fn calc_matrix(&self) -> Matrix4<f32> {
        OPENGL_TO_WGPU_MATRIX * perspective(self.fovy, self.aspect, self.znear, self.zfar)
    }
}
