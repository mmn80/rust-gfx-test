// There's a decent amount of code that's just for example and isn't called
#![allow(dead_code)]

use input::InputState;
use legion::*;
use scenes::SceneManagerAction;
use structopt::StructOpt;

use rafx::api::{RafxExtents2D, RafxResult, RafxSwapchainHelper};
use rafx::assets::AssetManager;
use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};

use crate::daemon_args::AssetDaemonArgs;
use crate::scenes::SceneManager;
use crate::time::TimeState;
use rafx::assets::distill_impl::AssetResource;
use rafx::nodes::ExtractResources;
use rafx::renderer::ViewportsResource;
use rafx::renderer::{AssetSource, Renderer};
use rafx::visibility::VisibilityRegion;

pub mod assets;
mod camera;
mod components;
pub mod daemon_args;
mod features;
mod init;
mod input;
mod phases;
mod render_graph_generator;
mod scenes;
mod time;

mod demo_plugin;

pub use demo_plugin::DemoRendererPlugin;

#[cfg(all(
    feature = "profile-with-tracy-memory",
    not(feature = "profile-with-stats-alloc")
))]
#[global_allocator]
static GLOBAL: profiling::tracy_client::ProfiledAllocator<std::alloc::System> =
    profiling::tracy_client::ProfiledAllocator::new(std::alloc::System, 100);

#[cfg(all(
    feature = "profile-with-stats-alloc",
    not(feature = "profile-with-tracy-memory")
))]
#[global_allocator]
pub static STATS_ALLOC: &stats_alloc::StatsAlloc<std::alloc::System> =
    &stats_alloc::INSTRUMENTED_SYSTEM;

struct StatsAllocMemoryRegion<'a> {
    region_name: &'a str,
    #[cfg(all(
        feature = "profile-with-stats-alloc",
        not(feature = "profile-with-tracy-memory")
    ))]
    region: stats_alloc::Region<'a, std::alloc::System>,
}

impl<'a> StatsAllocMemoryRegion<'a> {
    pub fn new(region_name: &'a str) -> Self {
        StatsAllocMemoryRegion {
            region_name,
            #[cfg(all(
                feature = "profile-with-stats-alloc",
                not(feature = "profile-with-tracy-memory")
            ))]
            region: stats_alloc::Region::new(STATS_ALLOC),
        }
    }
}

#[cfg(all(
    feature = "profile-with-stats-alloc",
    not(feature = "profile-with-tracy-memory")
))]
impl Drop for StatsAllocMemoryRegion<'_> {
    fn drop(&mut self) {
        log::info!(
            "({}) | {:?}",
            self.region_name,
            self.region.change_and_reset()
        );
    }
}

// Should be kept in sync with the constants in tonemapper.glsl
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub enum TonemapperType {
    None,
    StephenHillACES,
    SimplifiedLumaACES,
    Hejl2015,
    Hable,
    FilmicALU,
    LogDerivative,
    VisualizeRGBMax,
    VisualizeLuma,
    MAX,
}
impl TonemapperType {
    pub fn display_name(&self) -> &'static str {
        match self {
            TonemapperType::None => "None",
            TonemapperType::StephenHillACES => "Stephen Hill ACES",
            TonemapperType::SimplifiedLumaACES => "SimplifiedLumaACES",
            TonemapperType::Hejl2015 => "Hejl 2015",
            TonemapperType::Hable => "Hable",
            TonemapperType::FilmicALU => "Filmic ALU (Hable)",
            TonemapperType::LogDerivative => "LogDerivative",
            TonemapperType::VisualizeRGBMax => "Visualize RGB Max",
            TonemapperType::VisualizeLuma => "Visualize RGB Luma",
            TonemapperType::MAX => "MAX_TONEMAPPER_VALUE",
        }
    }
}
impl From<i32> for TonemapperType {
    fn from(v: i32) -> Self {
        assert!(v <= Self::MAX as i32);
        unsafe { std::mem::transmute(v) }
    }
}

impl std::fmt::Display for TonemapperType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}
#[derive(Clone)]
pub struct RenderOptions {
    pub enable_msaa: bool,
    pub enable_hdr: bool,
    pub enable_bloom: bool,
    pub show_debug3d: bool,
    pub show_text: bool,
    pub show_feature_toggles: bool,
    pub show_shadows: bool,
    pub blur_pass_count: usize,
    pub tonemapper_type: TonemapperType,
}

impl RenderOptions {
    fn default_2d() -> Self {
        RenderOptions {
            enable_msaa: false,
            enable_hdr: false,
            enable_bloom: false,
            show_debug3d: true,
            show_text: true,
            show_shadows: true,
            show_feature_toggles: false,
            blur_pass_count: 0,
            tonemapper_type: TonemapperType::None,
        }
    }

    fn default_3d() -> Self {
        RenderOptions {
            enable_msaa: true,
            enable_hdr: true,
            enable_bloom: true,
            show_debug3d: false,
            show_text: true,
            show_shadows: true,
            show_feature_toggles: true,
            blur_pass_count: 5,
            tonemapper_type: TonemapperType::LogDerivative,
        }
    }
}

impl RenderOptions {
    #[cfg(feature = "use-imgui")]
    pub fn window(&mut self, ui: &imgui::Ui<'_>) -> bool {
        let mut open = true;
        //TODO: tweak this and use imgui-inspect
        imgui::Window::new(imgui::im_str!("Render Options"))
            //.position([10.0, 25.0], imgui::Condition::FirstUseEver)
            //.size([600.0, 250.0], imgui::Condition::FirstUseEver)
            .opened(&mut open)
            .build(ui, || self.ui(ui));
        open
    }

    #[cfg(feature = "use-imgui")]
    pub fn ui(&mut self, ui: &imgui::Ui<'_>) {
        ui.checkbox(imgui::im_str!("enable_msaa"), &mut self.enable_msaa);
        ui.checkbox(imgui::im_str!("enable_hdr"), &mut self.enable_hdr);
        ui.checkbox(imgui::im_str!("enable_bloom"), &mut self.enable_bloom);

        if self.show_feature_toggles {
            ui.checkbox(
                imgui::im_str!("show_debug3d_feature"),
                &mut self.show_debug3d,
            );
            ui.checkbox(imgui::im_str!("show_text_feature"), &mut self.show_text);
            ui.checkbox(imgui::im_str!("show_shadows"), &mut self.show_shadows);
        }

        let mut blur_pass_count = self.blur_pass_count as i32;

        imgui::Drag::new(imgui::im_str!("blur_pass_count"))
            .range(0..=10)
            .build(ui, &mut blur_pass_count);

        self.blur_pass_count = blur_pass_count as usize;

        // iterate over the valid tonemapper values and convert them into their names
        let tonemapper_names: Vec<imgui::ImString> = (0..(TonemapperType::MAX as i32))
            .map(|t| imgui::ImString::new(TonemapperType::from(t).display_name()))
            .collect();

        let mut current_tonemapper_type = self.tonemapper_type as i32;

        if let Some(combo) = imgui::ComboBox::new(imgui::im_str!("tonemapper_type"))
            .preview_value(&tonemapper_names[current_tonemapper_type as usize])
            .begin(ui)
        {
            ui.list_box(
                imgui::im_str!(""),
                &mut current_tonemapper_type,
                &tonemapper_names.iter().collect::<Vec<_>>(),
                tonemapper_names.len() as i32,
            );
            combo.end(ui);
            self.tonemapper_type = current_tonemapper_type.into();
        }
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

pub fn run(args: &DemoArgs) -> RafxResult<()> {
    #[cfg(feature = "profile-with-tracy")]
    profiling::tracy_client::set_thread_name("Main Thread");
    #[cfg(feature = "profile-with-optick")]
    profiling::optick::register_thread("Main Thread");

    let mut scene_manager = SceneManager::default();

    let mut resources = Resources::default();
    resources.insert(TimeState::new());
    resources.insert(RenderOptions::default_2d());
    resources.insert(DebugUiState::default());
    resources.insert(InputState::new());

    let asset_source = if let Some(packfile) = &args.packfile {
        AssetSource::Packfile(packfile.to_path_buf())
    } else {
        AssetSource::Daemon {
            external_daemon: args.external_daemon,
            daemon_args: args.daemon_args.clone().into(),
        }
    };

    // Create the winit event loop
    let event_loop = winit::event_loop::EventLoop::<()>::with_user_event();
    let window = init::window_init(&event_loop);
    init::rendering_init(&mut resources, &window, asset_source)?;

    let mut world = World::default();
    let mut print_time_event = crate::time::PeriodicEvent::default();

    #[cfg(feature = "profile-with-puffin")]
    let mut profiler_ui = puffin_imgui::ProfilerUi::default();

    let mut scene_action = SceneManagerAction::Scene(scenes::Scene::Menu);

    event_loop.run(move |event, _window_target, control_flow| {
        {
            #[cfg(feature = "use-imgui")]
            let imgui_manager = resources
                .get::<crate::features::imgui::ImguiManager>()
                .unwrap();
            #[cfg(feature = "use-imgui")]
            imgui_manager.handle_event(&window, &event);
        }

        {
            let mut input = resources.get_mut::<InputState>().unwrap();
            if input
                .key_trigger
                .contains(&winit::event::VirtualKeyCode::Return)
                && input
                    .key_pressed
                    .contains(&winit::event::VirtualKeyCode::LAlt)
            {
                input
                    .key_trigger
                    .remove(&winit::event::VirtualKeyCode::Return);
                if let None = window.fullscreen() {
                    window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
                } else {
                    window.set_fullscreen(None);
                }
            }
        }

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,

            Event::LoopDestroyed => {
                init::rendering_destroy(&mut resources).unwrap();
            }

            Event::WindowEvent {
                event: window_event,
                ..
            } => {
                resources
                    .get_mut::<InputState>()
                    .unwrap()
                    .update(&window_event);
            }

            Event::MainEventsCleared => {
                window.request_redraw();
            }

            Event::RedrawRequested(_window_id) => {
                profiling::scope!("Main Loop");

                {
                    let mut viewports_resource = resources.get_mut::<ViewportsResource>().unwrap();
                    let window_size = window.inner_size();
                    if window_size.width > 0 && window_size.height > 0 {
                        viewports_resource.main_window_size = RafxExtents2D {
                            width: window_size.width,
                            height: window_size.height,
                        }
                    }
                }

                if let SceneManagerAction::Scene(scene) = scene_action {
                    scene_manager.try_load_scene(&mut world, &resources, scene);
                }

                let t0 = std::time::Instant::now();

                {
                    resources.get_mut::<TimeState>().unwrap().update();
                }

                {
                    let time_state = resources.get::<TimeState>().unwrap();
                    if print_time_event.try_take_event(
                        time_state.current_instant(),
                        std::time::Duration::from_secs_f32(1.0),
                    ) {
                        log::info!("FPS: {}", time_state.updates_per_second());
                        //renderer.dump_stats();
                    }
                }

                {
                    profiling::scope!("update asset resource");
                    let mut asset_resource = resources.get_mut::<AssetResource>().unwrap();
                    asset_resource.update();
                }

                {
                    profiling::scope!("update asset loaders");
                    let mut asset_manager = resources.get_mut::<AssetManager>().unwrap();
                    asset_manager.update_asset_loaders().unwrap();
                }

                #[cfg(feature = "use-imgui")]
                {
                    use crate::features::imgui::ImguiManager;
                    let imgui_manager = resources.get::<ImguiManager>().unwrap();
                    imgui_manager.begin_frame(&window);
                }

                {
                    scene_action = scene_manager.update_scene(&mut world, &mut resources);
                    if scene_action == SceneManagerAction::Exit {
                        *control_flow = ControlFlow::Exit
                    }
                }

                //
                // imgui debug draw,
                //
                #[cfg(feature = "use-imgui")]
                imgui_debug_draw(&resources, &mut profiler_ui);

                //
                // Close imgui input for this frame and render the results to memory
                //
                #[cfg(feature = "use-imgui")]
                {
                    use crate::features::imgui::ImguiManager;
                    let imgui_manager = resources.get::<ImguiManager>().unwrap();
                    imgui_manager.render(&window);
                }

                let t1 = std::time::Instant::now();
                log::trace!(
                    "[main] Simulation took {} ms",
                    (t1 - t0).as_secs_f32() * 1000.0
                );

                //
                // Redraw
                //
                {
                    profiling::scope!("Start Next Frame Render");
                    let game_renderer = resources.get::<Renderer>().unwrap();

                    let mut extract_resources = ExtractResources::default();

                    macro_rules! add_to_extract_resources {
                        ($ty: ident) => {
                            #[allow(non_snake_case)]
                            let mut $ty = resources.get_mut::<$ty>().unwrap();
                            extract_resources.insert(&mut *$ty);
                        };
                        ($ty: path, $name: ident) => {
                            let mut $name = resources.get_mut::<$ty>().unwrap();
                            extract_resources.insert(&mut *$name);
                        };
                    }

                    add_to_extract_resources!(VisibilityRegion);
                    add_to_extract_resources!(RafxSwapchainHelper);
                    add_to_extract_resources!(ViewportsResource);
                    add_to_extract_resources!(AssetManager);
                    add_to_extract_resources!(TimeState);
                    add_to_extract_resources!(RenderOptions);
                    add_to_extract_resources!(
                        crate::features::mesh::MeshRenderNodeSet,
                        mesh_render_node_set
                    );
                    add_to_extract_resources!(
                        crate::features::debug3d::DebugDraw3DResource,
                        debug_draw_3d_resource
                    );
                    add_to_extract_resources!(crate::features::text::TextResource, text_resource);
                    #[cfg(feature = "use-imgui")]
                    add_to_extract_resources!(crate::features::imgui::ImguiManager, imgui_manager);

                    let mut camera = resources.get_mut::<camera::RTSCamera>().unwrap();
                    extract_resources.insert(&mut *camera);

                    extract_resources.insert(&mut world);

                    game_renderer
                        .start_rendering_next_frame(&mut extract_resources)
                        .unwrap();
                }

                let t2 = std::time::Instant::now();
                log::trace!(
                    "[main] start rendering took {} ms",
                    (t2 - t1).as_secs_f32() * 1000.0
                );

                {
                    resources.get_mut::<InputState>().unwrap().clear();
                }

                profiling::finish_frame!();
            }

            _ => {}
        }
    });
}

fn imgui_debug_draw(resources: &Resources, profiler_ui: &mut puffin_imgui::ProfilerUi) {
    use crate::features::imgui::ImguiManager;
    profiling::scope!("imgui");
    let imgui_manager = resources.get::<ImguiManager>().unwrap();
    let time_state = resources.get::<TimeState>().unwrap();
    let mut debug_ui_state = resources.get_mut::<DebugUiState>().unwrap();
    let mut render_options = resources.get_mut::<RenderOptions>().unwrap();
    let asset_manager = resources.get::<AssetResource>().unwrap();
    imgui_manager.with_ui(|ui| {
        profiling::scope!("main menu bar");
        ui.main_menu_bar(|| {
            ui.menu(imgui::im_str!("Windows"), true, || {
                ui.checkbox(
                    imgui::im_str!("Render Options"),
                    &mut debug_ui_state.show_render_options,
                );

                ui.checkbox(
                    imgui::im_str!("Asset List"),
                    &mut debug_ui_state.show_asset_list,
                );

                #[cfg(feature = "profile-with-puffin")]
                if ui.checkbox(
                    imgui::im_str!("Profiler"),
                    &mut debug_ui_state.show_profiler,
                ) {
                    log::info!(
                        "Setting puffin profiler enabled: {:?}",
                        debug_ui_state.show_profiler
                    );
                    profiling::puffin::set_scopes_on(debug_ui_state.show_profiler);
                }
            });
            ui.text(imgui::im_str!(
                "FPS: {:.1}",
                time_state.updates_per_second_smoothed()
            ));
            ui.separator();
            ui.text(imgui::im_str!("Frame: {}", time_state.update_count()));
        });

        if debug_ui_state.show_render_options {
            imgui::Window::new(imgui::im_str!("Render Options")).build(ui, || {
                render_options.window(ui);
            });
        }

        if debug_ui_state.show_asset_list {
            imgui::Window::new(imgui::im_str!("Asset List"))
                .opened(&mut debug_ui_state.show_asset_list)
                .build(ui, || {
                    let loader = asset_manager.loader();
                    let mut asset_info = loader
                        .get_active_loads()
                        .into_iter()
                        .map(|item| loader.get_load_info(item))
                        .collect::<Vec<_>>();
                    asset_info.sort_by(|x, y| {
                        x.as_ref()
                            .map(|x| &x.path)
                            .cmp(&y.as_ref().map(|y| &y.path))
                    });
                    for info in asset_info {
                        if let Some(info) = info {
                            let id = info.asset_id;
                            ui.text(format!(
                                "{}:{} .. {}",
                                info.file_name.unwrap_or_else(|| "???".to_string()),
                                info.asset_name.unwrap_or_else(|| format!("{}", id)),
                                info.refs
                            ));
                        } else {
                            ui.text("NO INFO");
                        }
                    }
                });
        }

        #[cfg(feature = "profile-with-puffin")]
        if debug_ui_state.show_profiler {
            profiling::scope!("puffin profiler");
            profiler_ui.window(ui);
        }
    });
}
