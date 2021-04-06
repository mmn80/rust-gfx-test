use std::iter;
use std::collections::HashMap;

use cgmath::prelude::*;
use wgpu::util::DeviceExt;
use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};

mod camera;
mod model;
mod texture;
mod instance;
mod input;

use model::{DrawModel, Vertex};

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_position: [f32; 4],
    view_proj: [[f32; 4]; 4],
}

impl Uniforms {
    fn new() -> Self {
        Self {
            view_position: [0.0; 4],
            view_proj: cgmath::Matrix4::identity().into(),
        }
    }

    fn update_view_proj(&mut self, camera: &camera::Camera, projection: &camera::Projection) {
        self.view_position = camera.position.to_homogeneous().into();
        self.view_proj = (projection.calc_matrix() * camera.calc_matrix()).into()
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Light {
    position: [f32; 3],
    // Due to uniforms requiring 16 byte (4 float) spacing, we need to use a padding field here
    _padding: u32,
    color: [f32; 3],
}

struct MainGpu {
    window: winit::window::Window,
    size: winit::dpi::PhysicalSize<u32>,
    surface: wgpu::Surface,
    device: wgpu::Device,
    sc_desc: wgpu::SwapChainDescriptor,
    swap_chain: wgpu::SwapChain,
    queue: wgpu::Queue,
}

impl MainGpu {
    async fn new(window: Window) -> Self {
        let size = window.inner_size();

        // The instance is a handle to our GPU
        // BackendBit::PRIMARY => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = wgpu::Instance::new(wgpu::BackendBit::PRIMARY);
        let surface = unsafe { instance.create_surface(&window) };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
            })
            .await
            .unwrap();
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                },
                None, // Trace path
            )
            .await
            .unwrap();

        let sc_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
            format: adapter.get_swap_chain_preferred_format(&surface),
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
        };

        let swap_chain = device.create_swap_chain(&surface, &sc_desc);

        Self {
            window,
            size,
            surface,
            device,
            sc_desc,
            swap_chain,
            queue,
        }
    }
}

struct State {
    gpu: MainGpu,
    input: input::InputState,
    proc_pipeline: wgpu::RenderPipeline,
    tex_pipeline: wgpu::RenderPipeline,
    camera: camera::Camera,
    projection: camera::Projection,
    uniforms: Uniforms,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    instances: Vec<instance::Instance>,
    #[allow(dead_code)]
    instance_buffer: wgpu::Buffer,
    depth_texture: texture::Texture,
    light: Light,
    light_buffer: wgpu::Buffer,
    light_bind_group: wgpu::BindGroup,
    models: HashMap<&'static str, model::Model>,
    rendered_model: &'static str,
}

fn create_render_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    color_format: wgpu::TextureFormat,
    depth_format: Option<wgpu::TextureFormat>,
    vertex_layouts: &[wgpu::VertexBufferLayout],
    vs_src: wgpu::ShaderModuleDescriptor,
    fs_src: wgpu::ShaderModuleDescriptor,
) -> wgpu::RenderPipeline {
    let vs_module = device.create_shader_module(&vs_src);
    let fs_module = device.create_shader_module(&fs_src);

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &vs_module,
            entry_point: "main",
            buffers: vertex_layouts,
        },
        fragment: Some(wgpu::FragmentState {
            module: &fs_module,
            entry_point: "main",
            targets: &[wgpu::ColorTargetState {
                format: color_format,
                alpha_blend: wgpu::BlendState::REPLACE,
                color_blend: wgpu::BlendState::REPLACE,
                write_mask: wgpu::ColorWrite::ALL,
            }],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: wgpu::CullMode::Back,
            // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
            polygon_mode: wgpu::PolygonMode::Fill,
        },
        depth_stencil: depth_format.map(|format| wgpu::DepthStencilState {
            format,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
            // Setting this to true requires Features::DEPTH_CLAMPING
            clamp_depth: false,
        }),
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
    })
}

impl State {
    async fn new(window: Window) -> Self {
        let gpu = MainGpu::new(window).await;

        let camera = camera::Camera::new((0.0, 50.0, 0.0));
        let projection =
            camera::Projection::new(gpu.sc_desc.width, gpu.sc_desc.height, cgmath::Deg(45.0), 0.1, 100.0);

        let mut uniforms = Uniforms::new();
        uniforms.update_view_proj(&camera, &projection);

        let uniform_buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        });

        let uniform_bind_group_layout =
            gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: Some("uniform_bind_group_layout"),
            });

        let uniform_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
            label: Some("uniform_bind_group"),
        });

        let (instances, instance_buffer) = instance::mk_instances(&gpu.device);

        let light = Light {
            position: [2.0, 2.0, 2.0],
            _padding: 0,
            color: [1.0, 1.0, 1.0],
        };

        let light_buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Light VB"),
            contents: bytemuck::cast_slice(&[light]),
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        });

        let light_bind_group_layout =
            gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: None,
            });

        let light_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &light_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: light_buffer.as_entire_binding(),
            }],
            label: None,
        });

        let depth_texture =
            texture::Texture::create_depth_texture(&gpu.device, &gpu.sc_desc, "depth_texture");

        let tex_bind_group_layout =
            gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::Sampler {
                            comparison: false,
                            filtering: true,
                        },
                        count: None,
                    },
                    // normal map
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::Sampler {
                            comparison: false,
                            filtering: true,
                        },
                        count: None,
                    },
                ],
                label: Some("texture_bind_group_layout"),
            });
        let tex_pipeline_layout =
            gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    &uniform_bind_group_layout,
                    &light_bind_group_layout,
                    &tex_bind_group_layout,
                ],
                push_constant_ranges: &[],
            });
        let tex_pipeline = create_render_pipeline(
            &gpu.device,
            &tex_pipeline_layout,
            gpu.sc_desc.format,
            Some(texture::Texture::DEPTH_FORMAT),
            &[model::ModelVertex::desc(), instance::InstanceRaw::desc()],
            wgpu::include_spirv!("shader/main.vert.spv"),
            wgpu::include_spirv!("shader/main.frag.spv"),
        );

        let proc_pipeline_layout =
            gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Proc Pipeline Layout"),
                bind_group_layouts: &[
                    &uniform_bind_group_layout,
                    &light_bind_group_layout,
                ],
                push_constant_ranges: &[],
            });

        let proc_pipeline = create_render_pipeline(
            &gpu.device,
            &proc_pipeline_layout,
            gpu.sc_desc.format,
            Some(texture::Texture::DEPTH_FORMAT),
            &[model::ModelVertex::desc(), instance::InstanceRaw::desc()],
            wgpu::include_spirv!("shader/proc.vert.spv"),
            wgpu::include_spirv!("shader/proc.frag.spv"),
        );

        let models = HashMap::new();
        let rendered_model = "";

        let input = input::InputState::new();

        Self {
            gpu,
            input,
            proc_pipeline,
            tex_pipeline,
            camera,
            projection,
            uniform_buffer,
            uniform_bind_group,
            uniforms,
            instances,
            instance_buffer,
            depth_texture,
            light,
            light_buffer,
            light_bind_group,
            models,
            rendered_model,
        }
    }

    fn load_model(&mut self, file_name: &'static str) {
        self.rendered_model = file_name;
        if self.models.contains_key(file_name) {
            println!("Model {:?} already loaded.", file_name);
            return;
        }
        let res_dir = std::path::Path::new(env!("OUT_DIR")).join("res");
        let now = std::time::Instant::now();
        let obj_model = model::Model::load(
            &self.gpu.device,
            &self.gpu.queue,
            &self.tex_pipeline.get_bind_group_layout(2),
            res_dir.join(file_name),
        )
        .unwrap();
        println!("Model {:?} loaded in {:?}", file_name, std::time::Instant::now() - now);
        self.models.insert(file_name, obj_model);
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            println!("Attempted resize to 0 size.");
            return;
        }
        self.projection.resize(new_size.width, new_size.height);
        self.gpu.size = new_size;
        self.gpu.sc_desc.width = new_size.width;
        self.gpu.sc_desc.height = new_size.height;
        self.gpu.swap_chain = self.gpu.device.create_swap_chain(&self.gpu.surface, &self.gpu.sc_desc);
        self.depth_texture =
            texture::Texture::create_depth_texture(&self.gpu.device, &self.gpu.sc_desc, "depth_texture");
    }

    fn process_events(&mut self) {
        if self.input.key_trigger.contains(&VirtualKeyCode::F1) {
            self.load_model("rounded-cube.obj");
        }
        else if self.input.key_trigger.contains(&VirtualKeyCode::F2) {
            self.load_model("cube.obj");
        }
    }

    fn update(&mut self, dt: std::time::Duration) {
        self.camera.update(dt, &self.input);
        self.uniforms
            .update_view_proj(&self.camera, &self.projection);
        self.gpu.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniforms]),
        );

        // Update the light
        let old_position: cgmath::Vector3<_> = self.light.position.into();
        self.light.position =
            (cgmath::Quaternion::from_axis_angle((0.0, 1.0, 0.0).into(), cgmath::Deg(1.0))
                * old_position)
                .into();
        self.gpu.queue
            .write_buffer(&self.light_buffer, 0, bytemuck::cast_slice(&[self.light]));
    }

    fn render(&mut self) -> Result<(), wgpu::SwapChainError> {
        let frame = self.gpu.swap_chain.get_current_frame()?.output;

        let mut encoder = self.gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: &frame.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: true,
                    },
                }],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachmentDescriptor {
                    attachment: &self.depth_texture.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: true,
                    }),
                    stencil_ops: None,
                }),
            });

            render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            let model = &self.models[self.rendered_model];
            render_pass.set_pipeline(
                if model.has_textures() { &self.tex_pipeline } else { &self.proc_pipeline });
            render_pass.draw_model_instanced(
                model,
                0..self.instances.len() as u32,
                &self.uniform_bind_group,
                &self.light_bind_group,
            );
        }
        self.gpu.queue.submit(iter::once(encoder.finish()));

        self.input.clear();

        Ok(())
    }
}

pub fn init() {
    env_logger::init();
    let event_loop = EventLoop::new();
    let title = "SIM RTS MMO";
    let window = winit::window::WindowBuilder::new()
        .with_title(title)
        .build(&event_loop)
        .unwrap();
    use futures::executor::block_on;
    let mut state = block_on(State::new(window));
    state.load_model("cube.obj");
    let mut last_render_time = std::time::Instant::now();
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        match event {
            Event::WindowEvent {
                ref event,
                window_id,
            } if window_id == state.gpu.window.id() => {
                state.input.update(event);
                match event {
                    WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                    WindowEvent::KeyboardInput { input, .. } => match input {
                        KeyboardInput {
                            state: ElementState::Pressed,
                            virtual_keycode: Some(VirtualKeyCode::Escape),
                            ..
                        } => {
                            *control_flow = ControlFlow::Exit;
                        }
                        _ => {}
                    },
                    WindowEvent::Resized(physical_size) => {
                        state.resize(*physical_size);
                    }
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        state.resize(**new_inner_size);
                    }
                    _ => {}
                }
                state.process_events();
            }
            Event::MainEventsCleared => state.gpu.window.request_redraw(),
            Event::RedrawRequested(_) => {
                let now = std::time::Instant::now();
                let dt = now - last_render_time;
                last_render_time = now;
                state.update(dt);
                match state.render() {
                    Ok(_) => {}
                    Err(wgpu::SwapChainError::Lost) => state.resize(state.gpu.size),
                    Err(wgpu::SwapChainError::OutOfMemory) => *control_flow = ControlFlow::Exit,
                    Err(e) => eprintln!("{:?}", e),
                }
            }
            _ => {}
        }
    });
}
