use crate::block_adapter::{BlockAdapter, BLOCK_SIZE};
use crate::diagnostics::Diagnostics;
use crate::{DEFAULT_CODE, QUEUE_CAPACITY};
use glicol::Engine;
use nice_plug::prelude::Buffer;
use rtrb::Consumer;
use std::sync::Arc;

fn normalize_glicol_code(code: &str) -> String {
    let normalized = code.replace("\r\n", "\n").replace('\r', "\n");
    let mut result = String::with_capacity(normalized.len());

    for line in normalized.split_inclusive('\n') {
        if let Some(content) = line.strip_suffix('\n') {
            result.push_str(content.trim_end());
            result.push('\n');
        } else {
            result.push_str(line.trim_end());
        }
    }

    result
}

pub struct AudioEngine {
    engine: Engine<BLOCK_SIZE>,
    updates: Consumer<String>,
    adapter: BlockAdapter,
    input_channels: usize,
    diagnostics: Arc<Diagnostics>,
    current_code: String,
}

impl AudioEngine {
    pub fn new(updates: Consumer<String>, diagnostics: Arc<Diagnostics>) -> Self {
        let mut engine = Engine::new();
        engine.livecoding = false;
        engine.update_with_code(DEFAULT_CODE);
        Self {
            engine,
            updates,
            adapter: BlockAdapter::default(),
            input_channels: 2,
            diagnostics,
            current_code: DEFAULT_CODE.into(),
        }
    }

    // Host calls initialize while suspended, including after restoring state.
    pub fn configure(&mut self, sample_rate: f32, code: &str) {
        // Discard old GUI submissions so they cannot overwrite restored state.
        while self.updates.pop().is_ok() {}
        self.engine = Engine::new();
        self.engine.livecoding = false;
        self.engine.set_sr(sample_rate.round() as usize);
        let code = normalize_glicol_code(code);
        self.engine.update_with_code(&code);
        self.current_code = code;
        self.diagnostics.clear_error();
        self.reset();
    }

    pub fn set_input_channels(&mut self, channels: usize) {
        self.input_channels = channels;
    }

    pub fn reset(&mut self) {
        self.adapter = BlockAdapter::default();
    }

    pub fn process(&mut self, buffer: &mut Buffer) {
        // Keep queue consumption bounded, even if the GUI keeps submitting.
        for _ in 0..QUEUE_CAPACITY {
            match self.updates.pop() {
                Ok(code) => {
                    let code = normalize_glicol_code(&code);
                    if code != self.current_code {
                        self.engine.update_with_code(&code);
                        self.current_code = code;
                        self.diagnostics.clear_error();
                    }
                }
                Err(_) => break,
            }
        }
        let engine = &mut self.engine;
        let diagnostics = &self.diagnostics;
        let mut input_peak = 0.0_f32;
        let mut output_peak = 0.0_f32;
        let samples = buffer.samples();
        let output_channels = buffer.channels();
        for mut channels in buffer.iter_samples() {
            let left = *channels.get_mut(0).expect("Main input required");
            let right = if self.input_channels == 1 {
                left
            } else {
                *channels.get_mut(1).expect("Stereo input required")
            };
            input_peak = input_peak.max(left.abs()).max(right.abs());
            let output = self.adapter.process_frame([left, right], |left, right| {
                let (rendered, report) = engine.next_block(vec![&left[..], &right[..]]);
                if report[0] != 0 {
                    diagnostics.report(&report);
                }
                let mut output = [[0.0; BLOCK_SIZE]; 2];
                for (channel, destination) in output.iter_mut().enumerate() {
                    if let Some(source) = rendered.get(channel) {
                        destination.copy_from_slice(source);
                    }
                }
                output
            });
            if output_channels == 1 {
                let mono = (output[0] + output[1]) * 0.5;
                *channels.get_mut(0).unwrap() = mono;
                output_peak = output_peak.max(mono.abs());
            } else {
                *channels.get_mut(0).unwrap() = output[0];
                *channels.get_mut(1).unwrap() = output[1];
                output_peak = output_peak.max(output[0].abs()).max(output[1].abs());
            }
        }
        diagnostics.meter(input_peak, output_peak, samples);
    }
}
