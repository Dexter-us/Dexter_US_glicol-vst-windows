use crate::{GlicolParams, TEST_TONE};
use baseview::{Size, WindowHandle, WindowOpenOptions, WindowScalePolicy};
use egui_baseview::{EguiWindow, Queue, RenderSettings, Settings};
use nice_plug::prelude::{Editor, GuiContext, ParentWindowHandle};
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use std::any::Any;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const WIDTH: u32 = 560;
const HEIGHT: u32 = 420;
const MAX_PHYSICAL_HEIGHT: f64 = 600.0;

pub struct GlicolEditor {
    params: Arc<GlicolParams>,
    scale: Mutex<f64>,
    open: Arc<AtomicBool>,
    refresh: Arc<AtomicBool>,
    state: Arc<Mutex<EditorState>>,
}

impl GlicolEditor {
    pub fn new(params: Arc<GlicolParams>) -> Self {
        Self {
            state: Arc::new(Mutex::new(EditorState::new(&params))),
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
        let state = self.state.clone();
        let window_state = state.clone();
        #[cfg(windows)]
        let native = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        #[cfg(windows)]
        let file_owner = native.clone();
        let refresh = self.refresh.clone();
        let (width, height) = self.size();
        let settings = Settings {
            window: WindowOpenOptions {
                title: format!("Glicol VST — VST3 {}", env!("CARGO_PKG_VERSION")),
                size: Size::new(width as f64, height as f64),
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
                let mut state = window_state.lock().expect("Editor state mutex poisoned");
                if refresh.swap(false, Ordering::AcqRel) {
                    state.code = params.code.lock().expect("Program mutex poisoned").clone();
                    state.error = None;
                }
                draw_editor(ctx, params, &mut state);
                if let Some(action) = state.file_request.take() {
                    #[cfg(windows)]
                    match crate::windows_files::post_request(
                        file_owner.load(Ordering::Acquire) as _,
                        action,
                    ) {
                        Ok(()) => state.file_busy = true,
                        Err(error) => state.file_status = Some(error),
                    }
                    #[cfg(not(windows))]
                    {
                        let _ = action;
                        state.file_status =
                            Some("Disk file dialogs are available in the Windows build.".into());
                    }
                }
            },
        );
        #[cfg(windows)]
        {
            if let Err(error) = crate::windows_focus::install(&window, state) {
                window.close();
                panic!("Cannot open a usable editor: {error}");
            }
            if let RawWindowHandle::Windows(handle) = window.raw_window_handle() {
                native.store(handle.hwnd as usize, Ordering::Release);
            }
        }
        self.open.store(true, Ordering::Release);
        Box::new(EditorHandle {
            window,
            open: self.open.clone(),
        })
    }

    fn size(&self) -> (u32, u32) {
        let scale = *self.scale.lock().unwrap();
        (
            WIDTH.min((900.0 / scale) as u32).max(1),
            HEIGHT.min((MAX_PHYSICAL_HEIGHT / scale) as u32).max(1),
        )
    }

    fn set_scale_factor(&self, factor: f32) -> bool {
        if !factor.is_finite() || !(0.5..=3.0).contains(&factor) {
            return false;
        }
        let mut scale = self.scale.lock().unwrap();
        if self.open.load(Ordering::Acquire) {
            return *scale == factor as f64;
        }
        *scale = factor as f64;
        true
    }

    fn param_value_changed(&self, _id: &str, _value: f32) {}
    fn param_modulation_changed(&self, _id: &str, _offset: f32) {}
    fn param_values_changed(&self) {
        self.refresh.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    #[test]
    fn draft_and_restore_program_survive_scale_recreation() {
        let plugin = crate::GlicolVst3::default();
        let editor = GlicolEditor::new(plugin.params.clone());
        let draft = "o: speed 2.0 >> seq  55 60 _90 _ 48__90 >> mul 0.8";
        {
            let mut state = editor.state.lock().unwrap();
            state.code = draft.into();
            state.previous_code = Some("unsent restore draft".into());
        }
        for scale in [1.5, 2.0, 1.0] {
            assert!(editor.set_scale_factor(scale));
            let state = editor.state.lock().unwrap();
            assert_eq!(state.code, draft);
            assert_eq!(state.previous_code.as_deref(), Some("unsent restore draft"));
        }
        assert_eq!(*plugin.params.code.lock().unwrap(), crate::DEFAULT_CODE);
    }
}

pub(crate) struct EditorState {
    pub code: String,
    #[cfg(test)]
    pub code_rect: egui::Rect,
    error: Option<&'static str>,
    previous_code: Option<String>,
    samples_seen: u64,
    last_audio: Instant,
    pub(crate) file_request: Option<crate::program_files::FileAction>,
    pub(crate) file_busy: bool,
    pub(crate) file_status: Option<String>,
    pub(crate) file_path: Option<PathBuf>,
    pub(crate) file_snapshot: Option<String>,
    #[cfg(test)]
    pub save_rect: egui::Rect,
    #[cfg(test)]
    pub load_rect: egui::Rect,
}

impl EditorState {
    pub fn new(params: &GlicolParams) -> Self {
        Self {
            code: params.code.lock().unwrap().clone(),
            #[cfg(test)]
            code_rect: egui::Rect::NOTHING,
            error: None,
            previous_code: None,
            samples_seen: 0,
            last_audio: Instant::now(),
            file_request: None,
            file_busy: false,
            file_status: None,
            file_path: None,
            file_snapshot: None,
            #[cfg(test)]
            save_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            load_rect: egui::Rect::NOTHING,
        }
    }

    pub(crate) fn apply_file(&mut self, path: PathBuf, text: String) {
        if self.previous_code.is_none() {
            self.previous_code = Some(self.code.clone());
        }
        self.code = text.clone();
        self.file_snapshot = Some(text);
        self.file_path = Some(path.clone());
        self.error = None;
        self.file_status = Some(format!("Loaded {} — click Run to apply.", path.display()));
    }
}

// Shared by the real editor and headless layout regression tests.
// Return control and code clip rectangles for tests.
pub(crate) fn draw_editor(
    ctx: &egui::CtxRef,
    params: &GlicolParams,
    state: &mut EditorState,
) -> (egui::Rect, egui::Rect) {
    let samples = params.diagnostics.processed_samples.load(Ordering::Acquire);
    if samples != state.samples_seen {
        state.samples_seen = samples;
        state.last_audio = Instant::now();
    }
    let active = samples > 0 && state.last_audio.elapsed() < Duration::from_secs(2);
    let (input, output) = params.diagnostics.peaks();
    let mut buttons_rect = egui::Rect::NOTHING;
    let mut code_clip = egui::Rect::NOTHING;
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.set_enabled(!state.file_busy);
        ui.label(format!(
            "Glicol VST · VST3 {} · 128-sample latency",
            env!("CARGO_PKG_VERSION")
        ));
        let buttons = ui.horizontal_wrapped(|ui| {
            let save = ui.button("Save text");
            #[cfg(test)]
            {
                state.save_rect = save.rect;
            }
            if save.clicked() {
                state.file_request = Some(crate::program_files::FileAction::Save);
            }
            let load = ui.button("Load text");
            #[cfg(test)]
            {
                state.load_rect = load.rect;
            }
            if load.clicked() {
                state.file_request = Some(crate::program_files::FileAction::Load);
            }
            if ui.button("Run").clicked() {
                state.error = params.submit(state.code.clone()).err();
            }
            if ui.button("Test tone").clicked() {
                // Audition without throwing away the user's unsent editor draft.
                state.error = params.submit(TEST_TONE.into()).err();
                if state.error.is_none() {
                    if state.previous_code.is_none() {
                        state.previous_code = Some(state.code.clone());
                    }
                    state.code = TEST_TONE.into();
                }
            }
            if ui.button("Mute").clicked() {
                state.error = params.submit("o: sig 0;".into()).err();
            }
            if ui.button("Restore code").clicked() {
                let restore = state
                    .previous_code
                    .clone()
                    .unwrap_or_else(|| "o: ~input;".into());
                state.error = params.submit(restore.clone()).err();
                if state.error.is_none() {
                    state.code = restore;
                    state.previous_code = None;
                }
            }
            if ui.button("Pass input").clicked() {
                let pass = "o: ~input;".to_owned();
                state.error = params.submit(pass.clone()).err();
                if state.error.is_none() {
                    state.code = pass;
                }
            }
        });
        buttons_rect = buttons.response.rect;
        ui.label(if active {
            format!("Audio running · IN {:.3} · OUT {:.3}", input, output)
        } else {
            "No audio callbacks · Start playback / enable Input Echo; check bypass.".into()
        });
        ui.label("Tone needs no input. Pass input needs a playing clip or Input Echo.");
        if let Some(status) = &state.file_status {
            egui::ScrollArea::vertical()
                .id_source("file-status")
                .max_height(36.0)
                .show(ui, |ui| {
                    ui.label(status);
                });
        }
        if let Some(error) = state.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        if let Some(error) = params.diagnostics.error() {
            egui::ScrollArea::vertical()
                .id_source("errors")
                .max_height(48.0)
                .show(ui, |ui| {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                });
        }
        ui.separator();
        // Bound the viewport, not just the number of TextEdit rows.
        // Keep the buttons/meters fixed while long programs scroll underneath.
        egui::ScrollArea::vertical()
            .id_source("code")
            .max_height(ui.available_height().max(1.0))
            .auto_shrink([false, false])
            .always_show_scroll(false)
            .show(ui, |ui| {
                code_clip = ui.clip_rect();
                let response = ui.add(
                    egui::TextEdit::multiline(&mut state.code)
                        .code_editor()
                        .desired_rows(4)
                        .lock_focus(true)
                        .desired_width(f32::INFINITY),
                );
                #[cfg(test)]
                {
                    state.code_rect = response.rect;
                }
                #[cfg(not(test))]
                let _ = response;
            });
    });
    (buttons_rect, code_clip)
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
