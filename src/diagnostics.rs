//! Lock-free audio-thread telemetry; strings are assembled only in the GUI.
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};

pub struct Diagnostics {
    pub processed_samples: AtomicU64,
    input_peak: AtomicU32,
    output_peak: AtomicU32,
    error_epoch: AtomicU64,
    error_kind: AtomicU8,
    error_text: [AtomicU8; 254],
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            processed_samples: AtomicU64::new(0),
            input_peak: AtomicU32::new(0),
            output_peak: AtomicU32::new(0),
            error_epoch: AtomicU64::new(0),
            error_kind: AtomicU8::new(0),
            error_text: std::array::from_fn(|_| AtomicU8::new(0)),
        }
    }
}

impl Diagnostics {
    pub fn meter(&self, input: f32, output: f32, samples: usize) {
        self.input_peak.store(input.to_bits(), Ordering::Relaxed);
        self.output_peak.store(output.to_bits(), Ordering::Relaxed);
        self.processed_samples
            .fetch_add(samples as u64, Ordering::Release);
    }

    pub fn peaks(&self) -> (f32, f32) {
        (
            f32::from_bits(self.input_peak.load(Ordering::Relaxed)),
            f32::from_bits(self.output_peak.load(Ordering::Relaxed)),
        )
    }

    // Single writer: the audio engine, including suspended initialization.
    pub fn report(&self, payload: &[u8; 256]) {
        self.error_epoch.fetch_add(1, Ordering::AcqRel);
        for (destination, source) in self.error_text.iter().zip(&payload[2..]) {
            destination.store(*source, Ordering::Relaxed);
        }
        self.error_kind.store(payload[0], Ordering::Relaxed);
        self.error_epoch.fetch_add(1, Ordering::Release);
    }

    pub fn clear_error(&self) {
        self.report(&[0; 256]);
    }

    pub fn error(&self) -> Option<String> {
        // Never block the engine or expose a partly written error string.
        for _ in 0..2 {
            let epoch = self.error_epoch.load(Ordering::Acquire);
            if epoch % 2 != 0 {
                continue;
            }
            let kind = self.error_kind.load(Ordering::Relaxed);
            let bytes: Vec<u8> = self
                .error_text
                .iter()
                .map(|byte| byte.load(Ordering::Relaxed))
                .take_while(|byte| *byte != 0)
                .collect();
            if epoch != self.error_epoch.load(Ordering::Acquire) {
                continue;
            }
            return if kind == 0 {
                None
            } else {
                Some(format!(
                    "Glicol error {kind}: {}",
                    String::from_utf8_lossy(&bytes)
                ))
            };
        }
        Some("Engine status is updating…".into())
    }
}
