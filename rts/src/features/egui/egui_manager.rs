use super::{EguiContextResource, EguiDrawData};
use egui::FontDefinitions;
use egui_winit_platform::{Platform, PlatformDescriptor};
use std::sync::{Arc, Mutex};

// Inner state for EguiManager, which will be protected by a Mutex. Mutex protection required since
// this object is Send but not Sync
struct EguiManagerInner {
    platform: Platform,
    start_time: std::time::Instant,

    // This is produced when calling render()
    font_atlas: Option<Arc<egui::Texture>>,
    clipped_meshes: Option<Vec<egui::epaint::ClippedMesh>>,
}

//TODO: Investigate usage of channels/draw lists
#[derive(Clone)]
pub struct EguiManager {
    inner: Arc<Mutex<EguiManagerInner>>,
}

// Wraps egui (and winit integration logic)
impl EguiManager {
    pub fn new(window: &winit::window::Window) -> Self {
        let mut font_definitions = FontDefinitions::default();

        // Can remove the default_fonts feature and use custom fonts instead
        font_definitions.font_data.insert(
            "mplus-1p".to_string(),
            std::borrow::Cow::Borrowed(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/fonts/mplus-1p-regular.ttf"
            ))),
        );
        font_definitions.font_data.insert(
            "feather".to_string(),
            std::borrow::Cow::Borrowed(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/fonts/feather.ttf"
            ))),
        );
        font_definitions.font_data.insert(
            "materialdesignicons".to_string(),
            std::borrow::Cow::Borrowed(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/fonts/materialdesignicons-webfont.ttf"
            ))),
        );

        font_definitions.fonts_for_family.insert(
            egui::FontFamily::Monospace,
            vec![
                "mplus-1p".to_owned(),
                "feather".to_owned(), // fallback for √ etc
                "materialdesignicons".to_owned(),
            ],
        );
        font_definitions.fonts_for_family.insert(
            egui::FontFamily::Proportional,
            vec![
                "mplus-1p".to_owned(),
                "feather".to_owned(),
                "materialdesignicons".to_owned(),
            ],
        );

        font_definitions.family_and_size.insert(
            egui::TextStyle::Small,
            (egui::FontFamily::Proportional, 12.0),
        );
        font_definitions.family_and_size.insert(
            egui::TextStyle::Body,
            (egui::FontFamily::Proportional, 14.0),
        );
        font_definitions.family_and_size.insert(
            egui::TextStyle::Button,
            (egui::FontFamily::Proportional, 16.0),
        );
        font_definitions.family_and_size.insert(
            egui::TextStyle::Heading,
            (egui::FontFamily::Proportional, 20.0),
        );
        font_definitions.family_and_size.insert(
            egui::TextStyle::Monospace,
            (egui::FontFamily::Monospace, 12.0),
        );

        let size = window.inner_size();
        let platform = Platform::new(PlatformDescriptor {
            physical_width: (size.width as f64 * window.scale_factor()) as u32,
            physical_height: (size.height as f64 * window.scale_factor()) as u32,
            scale_factor: window.scale_factor(),
            font_definitions,
            style: egui::Style::default(),
        });

        EguiManager {
            inner: Arc::new(Mutex::new(EguiManagerInner {
                platform,
                start_time: std::time::Instant::now(),
                font_atlas: None,
                clipped_meshes: None,
            })),
        }
    }

    pub fn context_resource(&self) -> EguiContextResource {
        EguiContextResource {
            egui_manager: self.clone(),
        }
    }

    // Call when a window event is received
    //TODO: Taking a lock per event sucks
    #[profiling::function]
    pub fn handle_event<T>(&self, event: &winit::event::Event<T>) {
        let mut guard = self.inner.lock().unwrap();
        let inner = &mut *guard;
        inner.platform.handle_event(&event);
    }

    pub fn ignore_event<T>(&self, event: &winit::event::Event<T>) -> bool {
        let mut guard = self.inner.lock().unwrap();
        let inner = &mut *guard;
        inner.platform.captures_event(&event)
    }

    // Start a new frame
    #[profiling::function]
    pub fn begin_frame(&self) {
        let mut inner_mutex_guard = self.inner.lock().unwrap();
        let inner = &mut *inner_mutex_guard;

        inner
            .platform
            .update_time(inner.start_time.elapsed().as_secs_f64());

        inner.platform.begin_frame();
    }

    #[profiling::function]
    pub fn end_frame(&self) {
        let mut inner_mutex_guard = self.inner.lock().unwrap();
        let inner = &mut *inner_mutex_guard;

        let (_output, clipped_shapes) = inner.platform.end_frame();

        let context = inner.platform.context();

        let clipped_meshes = context.tessellate(clipped_shapes);

        //inner.output = Some(output);
        inner.clipped_meshes = Some(clipped_meshes);

        let mut new_texture = None;
        if let Some(texture) = &inner.font_atlas {
            if texture.version != context.texture().version {
                new_texture = Some(context.texture().clone());
            }
        } else {
            new_texture = Some(context.texture().clone());
        }

        if new_texture.is_some() {
            inner.font_atlas = new_texture.clone();
        }
    }

    pub fn context(&self) -> egui::CtxRef {
        let guard = self.inner.lock().unwrap();
        guard.platform.context()
    }

    #[profiling::function]
    pub fn take_draw_data(&self) -> Option<EguiDrawData> {
        let mut inner = self.inner.lock().unwrap();

        let clipped_meshes = inner.clipped_meshes.take();

        EguiDrawData::try_create_new(
            clipped_meshes?,
            inner.font_atlas.as_ref()?.clone(),
            inner.platform.context().pixels_per_point(),
        )
    }
}
