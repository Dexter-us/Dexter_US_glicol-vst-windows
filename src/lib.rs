//! Barebones baseview egui plugin

#[macro_use]
extern crate vst;

mod block_adapter;
#[cfg(test)]
mod tests;
#[cfg(target_os = "windows")]
mod windows_focus;

use egui;
use egui::CtxRef;

use baseview::{Size, WindowHandle, WindowOpenOptions, WindowScalePolicy};
use vst::buffer::AudioBuffer;
use vst::editor::Editor;
use vst::plugin::{Category, Info, Plugin, PluginParameters};

use egui_baseview::{EguiWindow, Queue, RenderSettings, Settings};
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};

use block_adapter::{BlockAdapter, BLOCK_SIZE};
use glicol::Engine;
use rtrb::{Consumer, Producer, RingBuffer};

use std::boxed::Box;
use std::sync::{Arc, Mutex};

const WINDOW_WIDTH: usize = 600;
const WINDOW_HEIGHT: usize = 800;
const DEFAULT_CODE: &str = "o: ~input >> mul 0.1;\n\n// Tone test: o: sin 440 >> mul 0.1;";

struct GlicolVSTPluginEditor {
    params: Arc<GlicolParams>,
    window_handle: Option<WindowHandle>,
    is_open: bool,
}

struct GlicolParams {
    code: Mutex<String>,
    updates: Mutex<Producer<String>>,
}

struct GlicolVSTPlugin {
    params: Arc<GlicolParams>,
    engine: Engine<BLOCK_SIZE>,
    updates: Consumer<String>,
    audio: BlockAdapter,
    editor: Option<GlicolVSTPluginEditor>,
}

struct VstParent(*mut ::std::ffi::c_void);

impl Editor for GlicolVSTPluginEditor {
    fn position(&self) -> (i32, i32) {
        (0, 0)
    }

    fn size(&self) -> (i32, i32) {
        (WINDOW_WIDTH as i32, WINDOW_HEIGHT as i32)
    }

    fn open(&mut self, parent: *mut ::std::ffi::c_void) -> bool {
        ::log::info!("Editor open");
        if self.is_open {
            return false;
        }

        let settings = Settings {
            window: WindowOpenOptions {
                title: String::from("Glicol VST"),
                size: Size::new(WINDOW_WIDTH as f64, WINDOW_HEIGHT as f64),
                scale: WindowScalePolicy::SystemScaleFactor,
            },
            render_settings: RenderSettings::default(),
        };

        let mut code = self.params.code.lock().unwrap().clone();
        let mut run_error = false;
        #[allow(unused_mut)]
        let mut window_handle = EguiWindow::open_parented(
            &VstParent(parent),
            settings,
            self.params.clone(),
            |_egui_ctx: &CtxRef, _queue: &mut Queue, _state: &mut Arc<GlicolParams>| {},
            move |egui_ctx: &CtxRef, _queue: &mut Queue, state: &mut Arc<GlicolParams>| {
                egui::CentralPanel::default().show(egui_ctx, |ui| {
                    ui.label("Glicol VST — stereo audio effect");
                    // Keep Run visible; the original 50-row editor extended
                    // below the 800-pixel native window on some displays.
                    if ui.button("Run").clicked() {
                        run_error = state.updates.lock().unwrap().push(code.clone()).is_err();
                        if !run_error {
                            *state.code.lock().unwrap() = code.clone();
                        }
                    }
                    if run_error {
                        ui.label("Update queue full. Enable audio processing, then click Run again.");
                    }
                    ui.label("Click the editor to type. Route audio into this effect, or run a tone test.");
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.add(
                        egui::TextEdit::multiline(&mut code).code_editor()
                        .desired_rows(50)
                        .lock_focus(true)
                        .desired_width(f32::INFINITY)
                        );
                    });
                });
            },
        );

        #[cfg(target_os = "windows")]
        if let Err(error) = windows_focus::install(&window_handle) {
            ::log::error!("{}", error);
            window_handle.close();
            return false;
        }
        self.window_handle = Some(window_handle);
        self.is_open = true;
        true
    }

    fn is_open(&mut self) -> bool {
        self.is_open
    }

    fn close(&mut self) {
        self.is_open = false;
        if let Some(mut window_handle) = self.window_handle.take() {
            window_handle.close();
        }
    }
}

impl Default for GlicolVSTPlugin {
    fn default() -> Self {
        let (producer, updates) = RingBuffer::new(4);
        let params = Arc::new(GlicolParams {
            code: Mutex::new(DEFAULT_CODE.to_owned()),
            updates: Mutex::new(producer),
        });
        let mut engine = Engine::<BLOCK_SIZE>::new();
        engine.update_with_code(DEFAULT_CODE);
        Self {
            params: params.clone(),
            engine: engine,
            updates,
            audio: BlockAdapter::default(),
            editor: Some(GlicolVSTPluginEditor {
                params: params.clone(),
                window_handle: None,
                is_open: false,
            }),
        }
    }
}

impl Plugin for GlicolVSTPlugin {
    fn get_info(&self) -> Info {
        Info {
            name: "Glicol VST".to_string(),
            vendor: "Dexter U.S.".to_string(),
            unique_id: 88886666,
            version: 2,
            inputs: 2,
            outputs: 2,
            parameters: 0,
            category: Category::Effect,
            initial_delay: BLOCK_SIZE as i32,
            ..Default::default()
        }
    }

    fn init(&mut self) {
        let log_folder = ::dirs::home_dir().unwrap().join("tmp");

        let _ = ::std::fs::create_dir(log_folder.clone());

        let log_file = ::std::fs::File::create(log_folder.join("GlicolVST.log")).unwrap();

        let log_config = ::simplelog::ConfigBuilder::new()
            .set_time_to_local(true)
            .build();

        let _ = ::simplelog::WriteLogger::init(simplelog::LevelFilter::Info, log_config, log_file);

        ::log_panics::init();

        ::log::info!("init");
    }

    fn get_editor(&mut self) -> Option<Box<dyn Editor>> {
        if let Some(editor) = self.editor.take() {
            Some(Box::new(editor) as Box<dyn Editor>)
        } else {
            None
        }
    }

    fn set_sample_rate(&mut self, rate: f32) {
        if !rate.is_finite() || rate <= 0.0 {
            ::log::error!("Host supplied invalid sample rate: {}", rate);
            return;
        }
        // set_sr affects newly constructed Glicol nodes. Rebuild while the
        // host is suspended so an existing oscillator also adopts the rate.
        self.engine = Engine::<BLOCK_SIZE>::new();
        self.engine.set_sr(rate.round() as usize);
        self.engine
            .update_with_code(&self.params.code.lock().unwrap());
        self.audio = BlockAdapter::default();
    }

    fn process(&mut self, buffer: &mut AudioBuffer<f32>) {
        // Transfer owned text, not a pointer into an editor String which may
        // be reallocated or destroyed while the audio thread reads it.
        for _ in 0..4 {
            match self.updates.pop() {
                Ok(code) => self.engine.update_with_code(&code),
                Err(_) => break,
            }
        }

        let block_size = buffer.samples();
        let (input, mut outputs) = buffer.split();
        let engine = &mut self.engine;
        for sample in 0..block_size {
            let left = if input.len() > 0 {
                input.get(0)[sample]
            } else {
                0.0
            };
            let right = if input.len() > 1 {
                input.get(1)[sample]
            } else {
                left
            };
            let frame = self.audio.process_frame([left, right], |left, right| {
                let engine_out = engine.next_block(vec![&left[..], &right[..]]).0;
                let mut block = [[0.0; BLOCK_SIZE]; 2];
                for (channel, dest) in block.iter_mut().enumerate() {
                    if let Some(source) = engine_out.get(channel) {
                        dest.copy_from_slice(source);
                    }
                }
                block
            });
            for channel in 0..outputs.len() {
                outputs.get_mut(channel)[sample] = frame.get(channel).copied().unwrap_or(0.0);
            }
        }
    }

    fn resume(&mut self) {
        self.audio = BlockAdapter::default();
    }

    fn get_parameter_object(&mut self) -> Arc<dyn PluginParameters> {
        Arc::clone(&self.params) as Arc<dyn PluginParameters>
    }
}

impl PluginParameters for GlicolParams {}

#[cfg(target_os = "macos")]
unsafe impl HasRawWindowHandle for VstParent {
    fn raw_window_handle(&self) -> RawWindowHandle {
        use raw_window_handle::macos::MacOSHandle;

        RawWindowHandle::MacOS(MacOSHandle {
            ns_view: self.0 as *mut ::std::ffi::c_void,
            ..MacOSHandle::empty()
        })
    }
}

#[cfg(target_os = "windows")]
unsafe impl HasRawWindowHandle for VstParent {
    fn raw_window_handle(&self) -> RawWindowHandle {
        use raw_window_handle::windows::WindowsHandle;

        RawWindowHandle::Windows(WindowsHandle {
            hwnd: self.0,
            ..WindowsHandle::empty()
        })
    }
}

#[cfg(target_os = "linux")]
unsafe impl HasRawWindowHandle for VstParent {
    fn raw_window_handle(&self) -> RawWindowHandle {
        use raw_window_handle::unix::XcbHandle;

        RawWindowHandle::Xcb(XcbHandle {
            window: self.0 as u32,
            ..XcbHandle::empty()
        })
    }
}

plugin_main!(GlicolVSTPlugin);
