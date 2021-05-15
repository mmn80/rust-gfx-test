use super::{ImGuiDrawData, ImguiFontAtlasData};
use imgui::sys as imgui_sys;
use imgui_winit_support::WinitPlatform;
use std::sync::Arc;
use std::sync::Mutex;
use winit::{event::Event, window::Window};

// Inner state for ImguiManager, which will be protected by a Mutex. Mutex protection required since
// this object is Send but not Sync
struct ImguiManagerInner {
    context: imgui::Context,

    // Pointer to the font atlas. Assuming no direct calls to C imgui interface, this pointer is
    // valid as long as context is not destroyed
    font_atlas_texture: *mut imgui::FontAtlasTexture<'static>,

    // Pointer to the current UI. Assuming no direct calls to C imgui interface, this pointer is
    // valid as long as context is not destroyed, and a frame has started and not ended
    ui: Option<*mut imgui::Ui<'static>>,

    platform: WinitPlatform,

    // These are all refreshed when frame is started
    want_capture_keyboard: bool,
    want_capture_mouse: bool,
    want_set_mouse_pos: bool,
    want_text_input: bool,
}

// Rust assumes pointers in Inner are not safe to send, so we need to explicitly impl that here
unsafe impl Send for ImguiManagerInner {}

impl Drop for ImguiManagerInner {
    fn drop(&mut self) {
        let mut ui = None;
        std::mem::swap(&mut self.ui, &mut ui);

        // Drop the UI call if it exists
        if let Some(ui) = ui {
            let _ui = unsafe { Box::from_raw(ui) };
        }

        // Drop the font atlas
        unsafe { Box::from_raw(self.font_atlas_texture) };
    }
}

//TODO: Investigate usage of channels/draw lists
#[derive(Clone)]
pub struct ImguiManager {
    inner: Arc<Mutex<ImguiManagerInner>>,
}

// Wraps imgui (and winit integration logic)
impl ImguiManager {
    // imgui and winit platform are expected to be pre-configured
    pub fn new(mut imgui_context: imgui::Context, platform: WinitPlatform) -> Self {
        // Ensure font atlas is built and cache a pointer to it
        let font_atlas_texture = {
            let mut fonts = imgui_context.fonts();
            let font_atlas_texture = Box::new(fonts.build_rgba32_texture());
            log::info!("Building ImGui font atlas");

            // Remove the lifetime of the texture. (We're assuming we have ownership over it
            // now since imgui_context is being passed to us)
            let font_atlas_texture: *mut imgui::FontAtlasTexture =
                Box::into_raw(font_atlas_texture);
            let font_atlas_texture: *mut imgui::FontAtlasTexture<'static> =
                unsafe { std::mem::transmute(font_atlas_texture) };
            font_atlas_texture
        };

        ImguiManager {
            inner: Arc::new(Mutex::new(ImguiManagerInner {
                context: imgui_context,
                font_atlas_texture,
                ui: None,
                platform,
                want_capture_keyboard: false,
                want_capture_mouse: false,
                want_set_mouse_pos: false,
                want_text_input: false,
            })),
        }
    }

    // Call when a winit event is received
    //TODO: Taking a lock per event sucks
    pub fn handle_event<T>(&self, window: &Window, event: &Event<T>) {
        let mut inner = self.inner.lock().unwrap();
        let inner = &mut *inner;
        let context = &mut inner.context;
        let platform = &mut inner.platform;

        platform.handle_event(context.io_mut(), &window, &event);
    }

    // Start a new frame
    #[profiling::function]
    pub fn begin_frame(&self, window: &Window) {
        let mut inner_mutex_guard = self.inner.lock().unwrap();
        let inner = &mut *inner_mutex_guard;

        // Drop the old Ui if it exists
        if inner.ui.is_some() {
            log::warn!("a frame is already in progress, starting a new one");
            Self::take_ui(&mut *inner);
        }

        inner
            .platform
            .prepare_frame(inner.context.io_mut(), window)
            .unwrap();
        let ui = Box::new(inner.context.frame());
        inner.want_capture_keyboard = ui.io().want_capture_keyboard;
        inner.want_capture_mouse = ui.io().want_capture_mouse;
        inner.want_set_mouse_pos = ui.io().want_set_mouse_pos;
        inner.want_text_input = ui.io().want_text_input;

        // Remove the lifetime of the Ui
        let ui_ptr: *mut imgui::Ui = Box::into_raw(ui);
        let ui_ptr: *mut imgui::Ui<'static> = unsafe { std::mem::transmute(ui_ptr) };

        // Store it as a raw pointer
        inner.ui = Some(ui_ptr);
    }

    fn take_ui(inner: &mut ImguiManagerInner) -> Option<Box<imgui::Ui<'static>>> {
        let mut ui = None;
        std::mem::swap(&mut inner.ui, &mut ui);

        if let Some(ui) = ui {
            return Some(unsafe { Box::from_raw(ui) });
        }

        None
    }

    #[profiling::function]
    pub fn render(&self, window: &Window) {
        let mut inner = self.inner.lock().unwrap();

        if inner.ui.is_none() {
            log::warn!("render() was called but a frame was not started");
            return;
        }

        let ui = ImguiManager::take_ui(&mut inner);
        if let Some(ui) = ui {
            inner.platform.prepare_render(&ui, window);
            ui.render();
        } else {
            log::warn!("ui did not exist");
        }
    }

    // Allows access to the context without caller needing to be aware of locking
    pub fn with_context<F>(&self, f: F)
    where
        F: FnOnce(&mut imgui::Context),
    {
        let mut inner = self.inner.lock().unwrap();
        (f)(&mut inner.context);
    }

    // Allows access to the ui without the caller needing to be aware of locking. A frame must be started
    pub fn with_ui<F>(&self, f: F)
    where
        F: FnOnce(&mut imgui::Ui),
    {
        let inner = self.inner.lock().unwrap();

        if inner.ui.is_none() {
            log::warn!("Tried to use imgui ui when a frame was not started");
            return;
        }

        if let Some(ui) = inner.ui {
            unsafe {
                (f)(&mut *ui);
            }
        }
    }

    #[profiling::function]
    pub fn build_font_atlas(&self) -> ImguiFontAtlasData {
        let mut font_atlas = None;
        self.with_context(|context| {
            let mut fonts = context.fonts();
            let font_atlas_texture = fonts.build_rgba32_texture();
            font_atlas = Some(ImguiFontAtlasData::new(&font_atlas_texture))
        });

        font_atlas.unwrap()
    }

    #[profiling::function]
    pub fn copy_font_atlas_texture(&self) -> Option<ImguiFontAtlasData> {
        let inner = self.inner.lock().unwrap();

        if inner.font_atlas_texture.is_null() {
            None
        } else {
            unsafe { Some(ImguiFontAtlasData::new(&*inner.font_atlas_texture)) }
        }
    }

    // Get reference to the underlying font atlas. The ref will be valid as long as this object
    // is not destroyed
    #[allow(unused_unsafe)]
    pub unsafe fn sys_font_atlas_texture(&self) -> Option<&imgui::FontAtlasTexture> {
        let inner = self.inner.lock().unwrap();

        if inner.font_atlas_texture.is_null() {
            None
        } else {
            unsafe { Some(&*inner.font_atlas_texture) }
        }
    }

    // Returns true if a frame has been started (and not ended)
    pub fn is_frame_started(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.ui.is_some()
    }

    #[profiling::function]
    pub fn copy_draw_data(&self) -> Option<ImGuiDrawData> {
        let inner = self.inner.lock().unwrap();

        if inner.ui.is_some() {
            log::warn!("get_draw_data() was called but a frame is in progress");
            return None;
        }

        let draw_data = unsafe { imgui_sys::igGetDrawData() };
        if draw_data.is_null() {
            log::warn!("no draw data available");
            return None;
        }

        let draw_data = unsafe { &*(draw_data as *mut imgui::DrawData) };
        Some(ImGuiDrawData::new(draw_data))
    }

    // Returns draw data (render must be called first to end the frame)
    #[allow(unused_unsafe)]
    pub unsafe fn sys_draw_data(&self) -> Option<&imgui::DrawData> {
        let inner = self.inner.lock().unwrap();

        if inner.ui.is_some() {
            log::warn!("get_draw_data() was called but a frame is in progress");
            return None;
        }

        let draw_data = unsafe { imgui_sys::igGetDrawData() };
        if draw_data.is_null() {
            log::warn!("no draw data available");
            return None;
        }

        let draw_data = unsafe { &*(draw_data as *mut imgui::DrawData) };
        Some(draw_data)
    }

    pub fn want_capture_keyboard(&self) -> bool {
        self.inner.lock().unwrap().want_capture_keyboard
    }

    pub fn want_capture_mouse(&self) -> bool {
        self.inner.lock().unwrap().want_capture_mouse
    }

    pub fn want_set_mouse_pos(&self) -> bool {
        self.inner.lock().unwrap().want_set_mouse_pos
    }

    pub fn want_text_input(&self) -> bool {
        self.inner.lock().unwrap().want_text_input
    }
}

fn init_imgui(window: &winit::window::Window) -> imgui::Context {
    use imgui::Context;

    let mut imgui = Context::create();

    {
        // Fix incorrect colors with sRGB framebuffer
        fn imgui_gamma_to_linear(col: [f32; 4]) -> [f32; 4] {
            let x = col[0].powf(2.2);
            let y = col[1].powf(2.2);
            let z = col[2].powf(2.2);
            let w = 1.0 - (1.0 - col[3]).powf(2.2);
            [x, y, z, w]
        }

        let style = imgui.style_mut();
        for col in 0..style.colors.len() {
            style.colors[col] = imgui_gamma_to_linear(style.colors[col]);
        }
    }

    imgui.set_ini_filename(None);

    // In the examples we only use integer DPI factors, because the UI can get very blurry
    // otherwise. This might or might not be what you want in a real application.
    let scale_factor = window.scale_factor().round();
    let font_size = (16.0 * scale_factor) as f32;

    let font_1p = imgui::FontSource::TtfData {
        data: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/fonts/mplus-1p-regular.ttf"
        )),
        size_pixels: font_size,
        config: None,
    };

    // Feather icons
    let font_feather = {
        const ICON_GLYPH_RANGE_FEATHER: [u16; 3] = [0xe81b, 0xe92a, 0];
        let mut font_config = imgui::FontConfig::default();
        font_config.glyph_ranges = imgui::FontGlyphRanges::from_slice(&ICON_GLYPH_RANGE_FEATHER);

        imgui::FontSource::TtfData {
            data: include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/fonts/feather.ttf"
            )),
            size_pixels: font_size,
            config: Some(font_config),
        }
    };

    let font_material = {
        // Material icons
        const ICON_GLYPH_RANGE_MATERIAL: [u16; 13] = [
            //0xfd24, 0xfd34, // transform/rotate icons
            0xf3e4, 0xf3e4, // pause
            0xf40a, 0xf40a, // play
            0xf1b5, 0xf1b5, // select
            0xfd25, 0xfd25, // translate
            0xfd74, 0xfd74, // rotate
            0xfa67, 0xfa67, // scale
            0,
        ];
        let mut font_config = imgui::FontConfig::default();
        font_config.glyph_ranges = imgui::FontGlyphRanges::from_slice(&ICON_GLYPH_RANGE_MATERIAL);
        font_config.glyph_offset = [0.0, 6.0];
        font_config.glyph_min_advance_x = 16.0;

        imgui::FontSource::TtfData {
            data: include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/fonts/materialdesignicons-webfont.ttf"
            )),
            size_pixels: font_size,
            config: Some(font_config),
        }
    };

    imgui
        .fonts()
        .add_font(&[font_1p, font_feather, font_material]);

    imgui.io_mut().font_global_scale = (1.0 / scale_factor) as f32;

    imgui
}

pub fn init_imgui_manager(window: &winit::window::Window) -> ImguiManager {
    let mut imgui_context = init_imgui(&window);
    let mut imgui_platform = imgui_winit_support::WinitPlatform::init(&mut imgui_context);

    imgui_platform.attach_window(
        imgui_context.io_mut(),
        &window,
        imgui_winit_support::HiDpiMode::Default,
    );

    ImguiManager::new(imgui_context, imgui_platform)
}
