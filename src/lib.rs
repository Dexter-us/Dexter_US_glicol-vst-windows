//! Separate VST3 port. The original VST2 source and class identity are untouched.
mod audio;
mod block_adapter;
mod diagnostics;
mod editor;
mod program_files;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod view_lifecycle_tests;
#[cfg(all(test, windows))]
mod windows_backend_tests;
#[cfg(windows)]
mod windows_files;
#[cfg(windows)]
mod windows_focus;

use audio::AudioEngine;
use block_adapter::BLOCK_SIZE;
use nice_plug::prelude::*;
use rtrb::{Producer, RingBuffer};
use std::sync::{Arc, Mutex};

pub const DEFAULT_CODE: &str = "o: ~input >> mul 0.1;";
pub const TEST_TONE: &str = "o: sin 440 >> mul 0.05;";
const QUEUE_CAPACITY: usize = 4;

#[derive(Params)]
pub struct GlicolParams {
    // Persist the last submitted program, not an unsent editor draft.
    #[persist = "glicol_code"]
    code: Mutex<String>,
    updates: Mutex<Producer<String>>,
    diagnostics: Arc<diagnostics::Diagnostics>,
}

impl GlicolParams {
    fn submit(&self, code: String) -> Result<(), &'static str> {
        let mut saved = self.code.lock().expect("Program mutex poisoned");
        self.updates
            .lock()
            .expect("Update queue mutex poisoned")
            .push(code.clone())
            .map_err(|_| "Update queue full. Enable audio processing, then click Run again.")?;
        *saved = code;
        Ok(())
    }
}

pub struct GlicolVst3 {
    params: Arc<GlicolParams>,
    audio: AudioEngine,
}

impl Default for GlicolVst3 {
    fn default() -> Self {
        let (updates, consumer) = RingBuffer::new(QUEUE_CAPACITY);
        let diagnostics = Arc::new(diagnostics::Diagnostics::default());
        Self {
            params: Arc::new(GlicolParams {
                code: Mutex::new(DEFAULT_CODE.into()),
                updates: Mutex::new(updates),
                diagnostics: diagnostics.clone(),
            }),
            audio: AudioEngine::new(consumer, diagnostics),
        }
    }
}

impl Plugin for GlicolVst3 {
    const NAME: &'static str = "Glicol VST";
    const VENDOR: &'static str = "Dexter U.S.";
    const URL: &'static str = "https://github.com/Dexter-us/glicol-vst-windows-private";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[],
            aux_output_ports: &[],
            names: PortNames::const_default(),
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
    ];
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        Some(Box::new(editor::GlicolEditor::new(self.params.clone())))
    }

    fn initialize(
        &mut self,
        layout: &AudioIOLayout,
        config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        if !Self::AUDIO_IO_LAYOUTS.iter().any(|supported| {
            supported.main_input_channels == layout.main_input_channels
                && supported.main_output_channels == layout.main_output_channels
        }) || !config.sample_rate.is_finite()
            || config.sample_rate <= 0.0
        {
            return false;
        }
        self.audio.configure(
            config.sample_rate,
            &self.params.code.lock().expect("Program mutex poisoned"),
        );
        self.audio
            .set_input_channels(layout.main_input_channels.unwrap().get() as usize);
        context.set_latency_samples(BLOCK_SIZE as u32);
        true
    }

    fn reset(&mut self) {
        self.audio.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.audio.process(buffer);
        // User code can be an oscillator even when the audio input is silent.
        // Do not let the host suspend a running generator as a zero-tail audio processor.
        ProcessStatus::KeepAlive
    }
}

impl Vst3Plugin for GlicolVst3 {
    // Stable new identity: never share a class ID with VST2 or another plugin.
    const VST3_CLASS_ID: [u8; 16] = *b"DexterGlicolVST3";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Instrument,
        Vst3SubCategory::Tools,
        Vst3SubCategory::Stereo,
    ];
}

nice_export_vst3!(GlicolVst3);
