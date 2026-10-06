use crate::block_adapter::{BlockAdapter, BLOCK_SIZE};
use crate::{DEFAULT_CODE, QUEUE_CAPACITY};
use glicol::Engine;
use nice_plug::prelude::Buffer;
use rtrb::Consumer;

pub struct AudioEngine {
    engine: Engine<BLOCK_SIZE>,
    updates: Consumer<String>,
    adapter: BlockAdapter,
}

impl AudioEngine {
    pub fn new(updates: Consumer<String>) -> Self {
        let mut engine = Engine::new();
        engine.update_with_code(DEFAULT_CODE);
        Self {
            engine,
            updates,
            adapter: BlockAdapter::default(),
        }
    }

    // Host calls initialize while suspended, including after restoring state.
    pub fn configure(&mut self, sample_rate: f32, code: &str) {
        // Discard old GUI submissions so they cannot overwrite restored state.
        while self.updates.pop().is_ok() {}
        self.engine = Engine::new();
        self.engine.set_sr(sample_rate.round() as usize);
        self.engine.update_with_code(code);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.adapter = BlockAdapter::default();
    }

    pub fn process(&mut self, buffer: &mut Buffer) {
        // Keep queue consumption bounded, even if the GUI keeps submitting.
        for _ in 0..QUEUE_CAPACITY {
            match self.updates.pop() {
                Ok(code) => self.engine.update_with_code(&code),
                Err(_) => break,
            }
        }
        let engine = &mut self.engine;
        for mut channels in buffer.iter_samples() {
            let left = *channels.get_mut(0).expect("Stereo input required");
            let right = *channels.get_mut(1).expect("Stereo input required");
            let output = self.adapter.process_frame([left, right], |left, right| {
                let rendered = engine.next_block(vec![&left[..], &right[..]]).0;
                let mut output = [[0.0; BLOCK_SIZE]; 2];
                for (channel, destination) in output.iter_mut().enumerate() {
                    if let Some(source) = rendered.get(channel) {
                        destination.copy_from_slice(source);
                    }
                }
                output
            });
            *channels.get_mut(0).expect("Stereo output required") = output[0];
            *channels.get_mut(1).expect("Stereo output required") = output[1];
        }
    }
}
