// There's a decent amount of code that's just for example and isn't called
#![allow(dead_code)]

use env::simulation::Simulation;
use legion::*;
use rafx::{
    api::{RafxExtents2D, RafxResult, RafxSwapchainHelper},
    assets::{distill_impl::AssetResource, AssetManager},
    base::memory::force_to_static_lifetime_mut,
    render_features::ExtractResources,
    renderer::{AssetSource, Renderer, RendererConfigResource, ViewportsResource},
};
use rafx_plugins::{
    features::{egui::WinitEguiManager, mesh::MeshRenderOptions},
    phases,
    pipelines::basic::{BasicPipelineRenderOptions, TonemapperType},
};
use structopt::StructOpt;
use time::PeriodicEvent;
use winit::{
    event::Event,
    event_loop::{ControlFlow, EventLoop},
    window::{Fullscreen, Window},
};

use crate::{
    camera::RTSCamera, daemon_args::AssetDaemonArgs, features::dyn_mesh::DynMeshManager,
    input::InputResource, scenes::SceneManager, scenes::SceneManagerAction, time::TimeState,
    ui::UiState,
};

mod assets;
mod camera;
pub mod daemon_args;
mod demo_renderer_thread_pool;
mod env;
mod features;
mod init;
mod input;
mod scenes;
mod time;
mod ui;
mod unit;

#[cfg(all(feature = "profile-with-tracy-memory", not(feature = "stats_alloc")))]
#[global_allocator]
static GLOBAL: profiling::tracy_client::ProfiledAllocator<std::alloc::System> =
    profiling::tracy_client::ProfiledAllocator::new(std::alloc::System, 100);

#[cfg(all(feature = "stats_alloc", not(feature = "profile-with-tracy-memory")))]
#[global_allocator]
pub static STATS_ALLOC: &stats_alloc::StatsAlloc<std::alloc::System> =
    &stats_alloc::INSTRUMENTED_SYSTEM;

struct StatsAllocMemoryRegion<'a> {
    region_name: &'a str,
    #[cfg(all(feature = "stats_alloc", not(feature = "profile-with-tracy-memory")))]
    region: stats_alloc::Region<'a, std::alloc::System>,
}

impl<'a> StatsAllocMemoryRegion<'a> {
    pub fn new(region_name: &'a str) -> Self {
        StatsAllocMemoryRegion {
            region_name,
            #[cfg(all(feature = "stats_alloc", not(feature = "profile-with-tracy-memory")))]
            region: stats_alloc::Region::new(STATS_ALLOC),
        }
    }
}

#[cfg(all(feature = "stats_alloc", not(feature = "profile-with-tracy-memory")))]
impl Drop for StatsAllocMemoryRegion<'_> {
    fn drop(&mut self) {
        log::info!(
            "({}) | {:?}",
            self.region_name,
            self.region.change_and_reset()
        );
    }
}

#[derive(Clone)]
pub struct RenderOptions {
    pub enable_msaa: bool,
    pub enable_hdr: bool,
    pub enable_bloom: bool,
    pub enable_textures: bool,
    pub enable_lighting: bool,
    pub show_surfaces: bool,
    pub show_wireframes: bool,
    pub show_debug3d: bool,
    pub show_text: bool,
    pub show_feature_toggles: bool,
    pub show_shadows: bool,
    pub blur_pass_count: usize,
    pub tonemapper_type: TonemapperType,
    pub enable_visibility_update: bool,
}

impl RenderOptions {
    fn default_2d() -> Self {
        RenderOptions {
            enable_msaa: false,
            enable_hdr: false,
            enable_bloom: false,
            enable_textures: true,
            enable_lighting: true,
            show_surfaces: true,
            show_wireframes: false,
            show_debug3d: true,
            show_text: true,
            show_shadows: true,
            show_feature_toggles: false,
            blur_pass_count: 0,
            tonemapper_type: TonemapperType::None,
            enable_visibility_update: true,
        }
    }

    fn default_3d() -> Self {
        RenderOptions {
            enable_msaa: true,
            enable_hdr: true,
            enable_bloom: true,
            enable_textures: true,
            enable_lighting: true,
            show_surfaces: true,
            show_wireframes: false,
            show_debug3d: true,
            show_text: true,
            show_shadows: true,
            show_feature_toggles: true,
            blur_pass_count: 5,
            tonemapper_type: TonemapperType::LogDerivative,
            enable_visibility_update: true,
        }
    }
}

impl RenderOptions {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.enable_msaa, "enable_msaa");
        ui.checkbox(&mut self.enable_hdr, "enable_hdr");

        if self.enable_hdr {
            ui.indent("HDR options", |ui| {
                let tonemapper_names: Vec<_> = (0..(TonemapperType::MAX as i32))
                    .map(|t| TonemapperType::from(t).display_name())
                    .collect();

                egui::ComboBox::from_label("tonemapper_type")
                    .selected_text(tonemapper_names[self.tonemapper_type as usize])
                    .show_ui(ui, |ui| {
                        for (i, name) in tonemapper_names.iter().enumerate() {
                            ui.selectable_value(
                                &mut self.tonemapper_type,
                                TonemapperType::from(i as i32),
                                name,
                            );
                        }
                    });

                ui.checkbox(&mut self.enable_bloom, "enable_bloom");
                if self.enable_bloom {
                    ui.indent("", |ui| {
                        ui.add(
                            egui::Slider::new(&mut self.blur_pass_count, 0..=10)
                                .clamp_to_range(true)
                                .text("blur_pass_count"),
                        );
                    });
                }
            });
        }

        if self.show_feature_toggles {
            ui.checkbox(&mut self.show_wireframes, "show_wireframes");
            ui.checkbox(&mut self.show_surfaces, "show_surfaces");

            if self.show_surfaces {
                ui.indent("", |ui| {
                    ui.checkbox(&mut self.enable_textures, "enable_textures");
                    ui.checkbox(&mut self.enable_lighting, "enable_lighting");

                    if self.enable_lighting {
                        ui.indent("", |ui| {
                            ui.checkbox(&mut self.show_shadows, "show_shadows");
                        });
                    }
                });
            }

            ui.checkbox(&mut self.show_debug3d, "show_debug3d_feature");
            ui.checkbox(&mut self.show_text, "show_text_feature");
        }

        ui.checkbox(
            &mut self.enable_visibility_update,
            "enable_visibility_update",
        );
    }
}

#[derive(Default)]
pub struct DebugUiState {
    show_render_options: bool,
    show_asset_list: bool,

    #[cfg(feature = "profile-with-puffin")]
    show_profiler: bool,
}

#[derive(StructOpt)]
pub struct DemoArgs {
    /// Path to the packfile
    #[structopt(name = "packfile", long, parse(from_os_str))]
    pub packfile: Option<std::path::PathBuf>,

    #[structopt(name = "external-daemon", long)]
    pub external_daemon: bool,

    #[structopt(flatten)]
    pub daemon_args: AssetDaemonArgs,
}

impl DemoArgs {
    fn asset_source(&self) -> AssetSource {
        if let Some(packfile) = &self.packfile {
            AssetSource::Packfile(packfile.to_path_buf())
        } else {
            AssetSource::Daemon {
                external_daemon: self.external_daemon,
                daemon_args: self.daemon_args.clone().into(),
            }
        }
    }
}

struct DemoApp {
    ui_state: UiState,
    scene_manager: SceneManager,
    resources: Resources,
    simulation: Simulation,
    print_time_event: PeriodicEvent,
}

impl DemoApp {
    fn init(args: &DemoArgs, window: &Window) -> RafxResult<Self> {
        #[cfg(feature = "profile-with-tracy")]
        profiling::tracy_client::set_thread_name("Main Thread");
        #[cfg(feature = "profile-with-optick")]
        profiling::optick::register_thread("Main Thread");

        let scene_manager = SceneManager::default();

        let mut resources = Resources::default();
        resources.insert(TimeState::new());
        resources.insert(RenderOptions::default_2d());
        resources.insert(MeshRenderOptions::default());
        resources.insert(BasicPipelineRenderOptions::default());
        resources.insert(DebugUiState::default());
        resources.insert(InputResource::new());

        let asset_source = args.asset_source();

        let physical_size = window.inner_size();
        init::rendering_init(
            &mut resources,
            asset_source,
            window,
            physical_size.width,
            physical_size.height,
        )?;

        let simulation = Simulation::new(&resources);
        let print_time_event = crate::time::PeriodicEvent::default();

        Ok(DemoApp {
            ui_state: Default::default(),
            scene_manager,
            resources,
            simulation,
            print_time_event,
        })
    }

    pub fn update(&mut self, window: &Window) -> RafxResult<ControlFlow> {
        profiling::scope!("Main Loop");

        let mut control_flow = ControlFlow::Poll;

        let t0 = rafx::base::Instant::now();

        {
            self.resources.get_mut::<TimeState>().unwrap().update();
        }

        {
            let time_state = self.resources.get::<TimeState>().unwrap();
            if self.print_time_event.try_take_event(
                time_state.current_instant(),
                std::time::Duration::from_secs_f32(1.0),
            ) {
                let fps = time_state.updates_per_second();
                if fps < 55. || fps > 65. {
                    log::info!("FPS: {}", time_state.updates_per_second());
                }
            }
        }

        {
            let mut viewports_resource = self.resources.get_mut::<ViewportsResource>().unwrap();
            let mut camera = self.resources.get_mut::<RTSCamera>().unwrap();
            let window_size = window.inner_size();
            if window_size.width > 0 && window_size.height > 0 {
                viewports_resource.main_window_size = RafxExtents2D {
                    width: window_size.width,
                    height: window_size.height,
                };
                camera.win_width = window_size.width;
                camera.win_height = window_size.height;
                camera.win_scale_factor = window.scale_factor() as f32;
            }
        }

        if let SceneManagerAction::Scene(scene) = self.scene_manager.scene_action {
            self.scene_manager
                .try_cleanup_current_scene(&mut self.simulation, &self.resources);

            {
                // NOTE(dvd): Legion leaks memory because the entity IDs aren't reset when the
                // world is cleared and the entity location map will grow without bounds.
                //self.simulation = World::default();

                // NOTE(dvd): The Renderer maintains some per-frame temporary data to avoid
                // allocating each frame. We can clear this between scene transitions.
                let mut renderer = self.resources.get_mut::<Renderer>().unwrap();
                renderer.clear_temporary_work();
            }
            self.scene_manager
                .try_load_scene(&mut self.simulation, &self.resources, scene);
        }

        {
            profiling::scope!("update asset resource");
            let mut asset_resource = self.resources.get_mut::<AssetResource>().unwrap();
            asset_resource.update();
        }

        {
            profiling::scope!("update asset loaders");
            let mut asset_manager = self.resources.get_mut::<AssetManager>().unwrap();
            asset_manager.update_asset_loaders().unwrap();
        }

        {
            profiling::scope!("update dyn mesh");
            let mut asset_manager = self.resources.get_mut::<AssetManager>().unwrap();
            let mut dyn_mesh_manager = self.resources.get_mut::<DynMeshManager>().unwrap();
            dyn_mesh_manager.update(&mut asset_manager);
        }

        {
            let egui_manager = self.resources.get::<WinitEguiManager>().unwrap();
            egui_manager.begin_frame(window)?;
        }

        {
            profiling::scope!("update scene");
            self.scene_manager.scene_action = self.scene_manager.update_scene(
                &mut self.simulation,
                &mut self.resources,
                &mut self.ui_state,
            );
            if self.scene_manager.scene_action == SceneManagerAction::Exit {
                control_flow = ControlFlow::Exit
            }
        }

        {
            let render_options = self.resources.get::<RenderOptions>().unwrap();
            let mut render_config_resource =
                self.resources.get_mut::<RendererConfigResource>().unwrap();
            render_config_resource
                .visibility_config
                .enable_visibility_update = render_options.enable_visibility_update;
            let mut basic_pipeline_render_options = self
                .resources
                .get_mut::<BasicPipelineRenderOptions>()
                .unwrap();
            basic_pipeline_render_options.enable_msaa = render_options.enable_msaa;
            basic_pipeline_render_options.enable_hdr = render_options.enable_hdr;
            basic_pipeline_render_options.enable_bloom = render_options.enable_bloom;
            basic_pipeline_render_options.enable_textures = render_options.enable_textures;
            basic_pipeline_render_options.show_surfaces = render_options.show_surfaces;
            basic_pipeline_render_options.show_wireframes = render_options.show_wireframes;
            basic_pipeline_render_options.show_debug3d = render_options.show_debug3d;
            basic_pipeline_render_options.show_text = render_options.show_text;
            basic_pipeline_render_options.show_skybox = false;
            basic_pipeline_render_options.show_feature_toggles =
                render_options.show_feature_toggles;
            basic_pipeline_render_options.blur_pass_count = render_options.blur_pass_count;
            basic_pipeline_render_options.tonemapper_type = render_options.tonemapper_type;
            basic_pipeline_render_options.enable_visibility_update =
                render_options.enable_visibility_update;

            let mut mesh_render_options = self.resources.get_mut::<MeshRenderOptions>().unwrap();
            mesh_render_options.show_surfaces = render_options.show_surfaces;
            mesh_render_options.show_shadows = render_options.show_shadows;
            mesh_render_options.enable_lighting = render_options.enable_lighting;
        }

        //
        // Close egui input for this frame
        //
        {
            let egui_manager = self.resources.get::<WinitEguiManager>().unwrap();
            egui_manager.end_frame();
        }

        let t1 = rafx::base::Instant::now();
        log::trace!(
            "[main] Simulation took {} ms",
            (t1 - t0).as_secs_f32() * 1000.0
        );

        //
        // Redraw
        //
        {
            profiling::scope!("Start next frame render");
            let renderer = self.resources.get::<Renderer>().unwrap();

            let mut extract_resources = ExtractResources::default();

            macro_rules! add_to_extract_resources {
                ($ty: ident) => {
                    #[allow(non_snake_case)]
                    let mut $ty = self.resources.get_mut::<$ty>().unwrap();
                    extract_resources.insert(&mut *$ty);
                };
                ($ty: path, $name: ident) => {
                    let mut $name = self.resources.get_mut::<$ty>().unwrap();
                    extract_resources.insert(&mut *$name);
                };
            }

            unsafe {
                let visibility_region = &mut self.simulation.universe().visibility_region;
                extract_resources.insert(force_to_static_lifetime_mut(visibility_region));
            }
            add_to_extract_resources!(RafxSwapchainHelper);
            add_to_extract_resources!(ViewportsResource);
            add_to_extract_resources!(AssetManager);
            add_to_extract_resources!(DynMeshManager);
            add_to_extract_resources!(TimeState);
            add_to_extract_resources!(RenderOptions);
            add_to_extract_resources!(BasicPipelineRenderOptions);
            add_to_extract_resources!(MeshRenderOptions);
            add_to_extract_resources!(RendererConfigResource);
            add_to_extract_resources!(
                rafx_plugins::features::mesh::MeshRenderObjectSet,
                mesh_render_object_set
            );
            add_to_extract_resources!(
                crate::features::dyn_mesh::DynMeshRenderObjectSet,
                dyn_mesh_render_object_set
            );
            add_to_extract_resources!(
                rafx_plugins::features::debug3d::Debug3DResource,
                debug_draw_3d_resource
            );
            add_to_extract_resources!(rafx_plugins::features::text::TextResource, text_resource);
            add_to_extract_resources!(WinitEguiManager, winit_egui_manager);
            let mut camera = self.resources.get_mut::<camera::RTSCamera>().unwrap();
            extract_resources.insert(&mut *camera);
            unsafe {
                let world = &mut self.simulation.universe().world;
                extract_resources.insert(force_to_static_lifetime_mut(world));
            }

            renderer
                .start_rendering_next_frame(&mut extract_resources)
                .unwrap();
        }

        let t2 = rafx::base::Instant::now();
        log::trace!(
            "[main] start rendering took {} ms",
            (t2 - t1).as_secs_f32() * 1000.0
        );

        profiling::finish_frame!();

        {
            let mut input_resource = self.resources.get_mut::<InputResource>().unwrap();
            input_resource.end_frame();
        }

        Ok(control_flow)
    }

    fn process_input(&mut self, event: &Event<()>, window: &Window) -> bool {
        Self::do_process_input(&self.resources, event, window)
    }

    fn do_process_input(resources: &Resources, event: &Event<()>, window: &Window) -> bool {
        use winit::event::*;

        let egui_manager = resources
            .get::<rafx_plugins::features::egui::WinitEguiManager>()
            .unwrap();

        let ignore_event = {
            egui_manager.handle_event(event);
            egui_manager.ignore_event(event)
        };

        if !ignore_event {
            //log::trace!("{:?}", event);
            let mut was_handled = false;
            match event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => return false,

                Event::WindowEvent {
                    event:
                        WindowEvent::KeyboardInput {
                            input:
                                KeyboardInput {
                                    state: ElementState::Pressed,
                                    virtual_keycode: Some(virtual_keycode),
                                    ..
                                },
                            ..
                        },
                    ..
                } => {
                    //log::trace!("Key Down {:?} {:?}", keycode, modifiers);

                    if *virtual_keycode == VirtualKeyCode::M {
                        let metrics = resources.get::<AssetManager>().unwrap().metrics();
                        println!("{:#?}", metrics);
                        was_handled = true;
                    }
                }
                _ => {}
            }

            if !was_handled {
                let mut input_resource = resources.get_mut::<InputResource>().unwrap();
                input::handle_winit_event(event, &mut *input_resource);

                if input_resource.is_key_just_up(input::KeyboardKey::Return)
                    && input_resource.is_key_down(input::KeyboardKey::LAlt)
                {
                    input_resource.end_frame();

                    if window.fullscreen().is_none() {
                        window.set_fullscreen(Some(Fullscreen::Borderless(None)));
                    } else {
                        window.set_fullscreen(None);
                    }
                }
            }
        }

        true
    }
}

impl Drop for DemoApp {
    fn drop(&mut self) {
        init::rendering_destroy(&mut self.resources).unwrap()
    }
}

pub fn update_loop(args: &DemoArgs, window: Window, event_loop: EventLoop<()>) -> RafxResult<()> {
    log::debug!("calling init");
    let mut app = DemoApp::init(args, &window).unwrap();

    log::debug!("start update loop");
    event_loop.run(move |event, _, control_flow| match event {
        Event::MainEventsCleared => {
            window.request_redraw();
        }
        Event::RedrawRequested(_) => {
            *control_flow = app.update(&window).unwrap();
        }
        event @ _ => {
            if !app.process_input(&event, &window) {
                *control_flow = ControlFlow::Exit;
            }
        }
    });
}
