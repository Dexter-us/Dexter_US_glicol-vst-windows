use crate::GlicolParams;
use baseview::{Size, WindowHandle, WindowOpenOptions, WindowScalePolicy};
use egui_baseview::{EguiWindow, Queue, RenderSettings, Settings};
use nice_plug::prelude::{Editor, GuiContext, ParentWindowHandle};
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const WIDTH: u32 = 600;
const HEIGHT: u32 = 800;

pub struct GlicolEditor {
    params: Arc<GlicolParams>,
    scale: Mutex<f64>,
    open: Arc<AtomicBool>,
    refresh: Arc<AtomicBool>,
}

impl GlicolEditor {
    pub fn new(params: Arc<GlicolParams>) -> Self {
        Self {
            params,
            scale: Mutex::new(1.0),
            open: Arc::new(AtomicBool::new(false)),
            refresh: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Editor for GlicolEditor {
    fn spawn(
        &self,
        parent: ParentWindowHandle,
        _context: Arc<dyn GuiContext>,
    ) -> Box<dyn Any + Send> {
        let mut code = self
            .params
            .code
            .lock()
            .expect("Program mutex poisoned")
            .clone();
        let mut error = None;
        let refresh = self.refresh.clone();
        let settings = Settings {
            window: WindowOpenOptions {
                title: "Glicol VST — VST3".into(),
                size: Size::new(WIDTH as f64, HEIGHT as f64),
                scale: WindowScalePolicy::ScaleFactor(*self.scale.lock().unwrap()),
            },
            render_settings: RenderSettings::default(),
        };
        #[allow(unused_mut)]
        let mut window = EguiWindow::open_parented(
            &LegacyParent(parent),
            settings,
            self.params.clone(),
            |_: &egui::CtxRef, _: &mut Queue, _: &mut Arc<GlicolParams>| {},
            move |ctx: &egui::CtxRef, _: &mut Queue, params: &mut Arc<GlicolParams>| {
                if refresh.swap(false, Ordering::AcqRel) {
                    code = params.code.lock().expect("Program mutex poisoned").clone();
                    error = None;
                }
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.label("Glicol VST — VST3 stereo audio effect");
                    if ui.button("Run").clicked() {
                        error = params.submit(code.clone()).err();
                    }
                    if let Some(error) = error {
                        ui.label(error);
                    }
                    ui.label("Click to type. Route audio into this effect, or run a quiet tone.");
                    ui.label("Latency: 128 samples. Submitted code is saved with the project.");
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut code)
                                .code_editor()
                                .desired_rows(50)
                                .lock_focus(true)
                                .desired_width(f32::INFINITY),
                        );
                    });
                });
            },
        );
        #[cfg(windows)]
        if let Err(error) = crate::windows_focus::install(&window) {
            window.close();
            panic!("Cannot open a usable editor: {error}");
        }
        self.open.store(true, Ordering::Release);
        Box::new(EditorHandle {
            window,
            open: self.open.clone(),
        })
    }

    fn size(&self) -> (u32, u32) {
        (WIDTH, HEIGHT)
    }

    fn set_scale_factor(&self, factor: f32) -> bool {
        if !factor.is_finite() || factor <= 0.0 || self.open.load(Ordering::Acquire) {
            return false;
        }
        *self.scale.lock().unwrap() = factor as f64;
        true
    }

    fn param_value_changed(&self, _id: &str, _value: f32) {}
    fn param_modulation_changed(&self, _id: &str, _offset: f32) {}
    fn param_values_changed(&self) {
        self.refresh.store(true, Ordering::Release);
    }
}

struct EditorHandle {
    window: WindowHandle,
    open: Arc<AtomicBool>,
}

// The framework creates and destroys editor handles on the host GUI thread.
// This matches nice-plug's own egui adapter; Send is required only for storage.
unsafe impl Send for EditorHandle {}

impl Drop for EditorHandle {
    fn drop(&mut self) {
        self.window.close();
        self.open.store(false, Ordering::Release);
    }
}

// Adapt nice-plug's raw-window-handle 0.5 parent to the existing GUI's 0.3 API.
struct LegacyParent(ParentWindowHandle);

unsafe impl HasRawWindowHandle for LegacyParent {
    fn raw_window_handle(&self) -> RawWindowHandle {
        match self.0 {
            #[cfg(windows)]
            ParentWindowHandle::Win32Hwnd(hwnd) => {
                RawWindowHandle::Windows(raw_window_handle::windows::WindowsHandle {
                    hwnd,
                    ..raw_window_handle::windows::WindowsHandle::empty()
                })
            }
            #[cfg(target_os = "macos")]
            ParentWindowHandle::AppKitNsView(ns_view) => {
                RawWindowHandle::MacOS(raw_window_handle::macos::MacOSHandle {
                    ns_view,
                    ..raw_window_handle::macos::MacOSHandle::empty()
                })
            }
            #[cfg(target_os = "linux")]
            ParentWindowHandle::X11Window(window) => {
                RawWindowHandle::Xcb(raw_window_handle::unix::XcbHandle {
                    window,
                    ..raw_window_handle::unix::XcbHandle::empty()
                })
            }
            _ => panic!("Unsupported parent window platform"),
        }
    }
}
