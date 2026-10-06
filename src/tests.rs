use super::*;
use std::cell::Cell;

fn run_buffer(audio: &mut AudioEngine, input: [Vec<f32>; 2]) -> [Vec<f32>; 2] {
    let [mut left, mut right] = input;
    let mut buffer = Buffer::default();
    // Both owned channel slices have identical length and outlive the Buffer.
    unsafe {
        buffer.set_slices(left.len(), |slices| {
            slices.clear();
            slices.push(&mut left);
            slices.push(&mut right);
        });
    }
    audio.process(&mut buffer);
    drop(buffer);
    [left, right]
}

#[test]
fn arbitrary_callbacks_preserve_stereo_and_latency() {
    let mut plugin = GlicolVst3::default();
    let mut position = 0;
    for _ in 0..4 {
        for size in [0, 1, 32, 64, 127, 128, 192, 257, 512] {
            let output = run_buffer(&mut plugin.audio, [vec![0.5; size], vec![-0.25; size]]);
            for sample in 0..size {
                let expected = if position < BLOCK_SIZE {
                    [0.0; 2]
                } else {
                    [0.05, -0.025]
                };
                for channel in 0..2 {
                    assert!((output[channel][sample] - expected[channel]).abs() < 0.00001);
                }
                position += 1;
            }
        }
    }
}

#[test]
fn submitted_tone_owns_text_even_after_editor_closes() {
    let mut plugin = GlicolVst3::default();
    let mut code = "o: sin 440 >> mul 0.1;".to_owned();
    plugin.params.submit(code.clone()).unwrap();
    code.clear();
    code.push_str(&"x".repeat(10000));
    drop(code);
    let output = run_buffer(&mut plugin.audio, [vec![0.0; 4096], vec![0.0; 4096]]);
    assert!(output.iter().flatten().all(|sample| sample.is_finite()));
    let peak = output[0].iter().fold(0.0_f32, |a, v| a.max(v.abs()));
    assert!(peak > 0.09 && peak <= 0.101);
}

#[test]
fn sample_rate_applies_to_existing_program() {
    for rate in [44100, 48000] {
        let mut plugin = GlicolVst3::default();
        plugin
            .audio
            .configure(rate as f32, "o: sin 440 >> mul 0.1;");
        let length = rate + BLOCK_SIZE;
        let output = run_buffer(&mut plugin.audio, [vec![0.0; length], vec![0.0; length]]);
        let crossings = output[0][BLOCK_SIZE..]
            .windows(2)
            .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
            .count();
        assert!((439..=441).contains(&crossings), "rate {rate}: {crossings}");
    }
}

#[test]
fn full_queue_does_not_replace_saved_program() {
    let plugin = GlicolVst3::default();
    for _ in 0..QUEUE_CAPACITY {
        plugin
            .params
            .submit("o: sin 440 >> mul 0.1;".into())
            .unwrap();
    }
    assert!(plugin.params.submit("o: sin 880;".into()).is_err());
    assert_eq!(
        *plugin.params.code.lock().unwrap(),
        "o: sin 440 >> mul 0.1;"
    );
}

#[test]
fn saved_program_roundtrips_and_discards_stale_queue() {
    let original = GlicolVst3::default();
    original
        .params
        .submit("o: sin 440 >> mul 0.1;".into())
        .unwrap();
    let fields = original.params.serialize_fields();
    let mut restored = GlicolVst3::default();
    restored
        .params
        .submit("o: sin 880 >> mul 0.1;".into())
        .unwrap();
    restored.params.deserialize_fields(&fields);
    restored
        .audio
        .configure(48000.0, &restored.params.code.lock().unwrap());
    let output = run_buffer(&mut restored.audio, [vec![0.0; 48128], vec![0.0; 48128]]);
    let crossings = output[0][BLOCK_SIZE..]
        .windows(2)
        .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
        .count();
    assert!((439..=441).contains(&crossings));
}

struct TestInit(Cell<u32>);

impl InitContext<GlicolVst3> for TestInit {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Vst3
    }
    fn execute(&self, _task: ()) {}
    fn set_latency_samples(&self, samples: u32) {
        self.0.set(samples);
    }
    fn set_current_voice_capacity(&self, _capacity: u32) {}
}

#[test]
fn initialization_declares_latency_and_rejects_wrong_layout() {
    let mut plugin = GlicolVst3::default();
    let mut context = TestInit(Cell::new(0));
    let mut config = BufferConfig {
        sample_rate: 48000.0,
        min_buffer_size: Some(32),
        max_buffer_size: 512,
        process_mode: ProcessMode::Realtime,
    };
    assert!(plugin.initialize(&GlicolVst3::AUDIO_IO_LAYOUTS[0], &config, &mut context));
    assert_eq!(context.0.get(), 128);
    let mut mono = GlicolVst3::AUDIO_IO_LAYOUTS[0];
    mono.main_input_channels = NonZeroU32::new(1);
    assert!(!plugin.initialize(&mono, &config, &mut context));
    config.sample_rate = f32::NAN;
    assert!(!plugin.initialize(&GlicolVst3::AUDIO_IO_LAYOUTS[0], &config, &mut context));
}

#[test]
fn exported_vst3_factory_exposes_an_audio_effect() {
    use vst3::ComPtr;
    use vst3::Steinberg::{kResultOk, IPluginFactoryTrait, PClassInfo};
    let factory = unsafe { ComPtr::from_raw(GetPluginFactory()) }.expect("VST3 factory missing");
    unsafe {
        assert_eq!(factory.countClasses(), 1);
        let mut info: PClassInfo = std::mem::zeroed();
        assert_eq!(factory.getClassInfo(0, &mut info), kResultOk);
        let bytes: Vec<u8> = info
            .category
            .iter()
            .map(|v| *v as u8)
            .take_while(|v| *v != 0)
            .collect();
        assert_eq!(bytes, b"Audio Module Class");
        let name: Vec<u8> = info
            .name
            .iter()
            .map(|v| *v as u8)
            .take_while(|v| *v != 0)
            .collect();
        assert_eq!(name, b"Glicol VST");
    }
}

#[test]
fn host_can_create_and_process_through_vst3_interfaces() {
    use std::ptr::null_mut;
    use vst3::Steinberg::Vst::{
        AudioBusBuffers, AudioBusBuffers__type0, IAudioProcessor, IAudioProcessorTrait, IComponent,
        IComponentTrait, IEditController, ProcessData, ProcessSetup,
    };
    use vst3::Steinberg::{kResultOk, IPluginBaseTrait, IPluginFactoryTrait, PClassInfo};
    use vst3::{ComPtr, Interface};

    // Exercise the actual exported factory and COM ABI, not just Rust DSP methods.
    unsafe {
        let factory = ComPtr::from_raw(GetPluginFactory()).unwrap();
        let mut info: PClassInfo = std::mem::zeroed();
        assert_eq!(factory.getClassInfo(0, &mut info), kResultOk);
        let mut instance = null_mut();
        assert_eq!(
            factory.createInstance(
                info.cid.as_ptr(),
                IComponent::IID.as_ptr().cast(),
                &mut instance,
            ),
            kResultOk,
        );
        let component = ComPtr::<IComponent>::from_raw(instance.cast()).unwrap();
        assert!(component.cast::<IEditController>().is_some());
        assert_eq!(component.initialize(null_mut()), kResultOk);
        let processor = component.cast::<IAudioProcessor>().unwrap();
        let mut setup = ProcessSetup {
            processMode: 0,        // kRealtime
            symbolicSampleSize: 0, // kSample32
            maxSamplesPerBlock: 512,
            sampleRate: 48000.0,
        };
        assert_eq!(processor.setupProcessing(&mut setup), kResultOk);
        assert_eq!(component.setActive(1), kResultOk);
        assert_eq!(processor.getLatencySamples(), 128);
        assert_eq!(processor.setProcessing(1), kResultOk);

        let mut position = 0;
        for size in [32, 64, 128, 192, 256, 512] {
            let mut input = [vec![0.5; size], vec![-0.25; size]];
            let mut output = [vec![f32::NAN; size], vec![f32::NAN; size]];
            let mut in_ptrs = [input[0].as_mut_ptr(), input[1].as_mut_ptr()];
            let mut out_ptrs = [output[0].as_mut_ptr(), output[1].as_mut_ptr()];
            let mut input_bus = AudioBusBuffers {
                numChannels: 2,
                silenceFlags: 0,
                __field0: AudioBusBuffers__type0 {
                    channelBuffers32: in_ptrs.as_mut_ptr(),
                },
            };
            let mut output_bus = AudioBusBuffers {
                numChannels: 2,
                silenceFlags: 0,
                __field0: AudioBusBuffers__type0 {
                    channelBuffers32: out_ptrs.as_mut_ptr(),
                },
            };
            let mut data = ProcessData {
                processMode: 0,
                symbolicSampleSize: 0,
                numSamples: size as i32,
                numInputs: 1,
                numOutputs: 1,
                inputs: &mut input_bus,
                outputs: &mut output_bus,
                inputParameterChanges: null_mut(),
                outputParameterChanges: null_mut(),
                inputEvents: null_mut(),
                outputEvents: null_mut(),
                processContext: null_mut(),
            };
            assert_eq!(processor.process(&mut data), kResultOk);
            for sample in 0..size {
                let expected = if position < BLOCK_SIZE {
                    [0.0; 2]
                } else {
                    [0.05, -0.025]
                };
                for channel in 0..2 {
                    assert!((output[channel][sample] - expected[channel]).abs() < 0.00001);
                }
                position += 1;
            }
        }
        assert_eq!(processor.setProcessing(0), kResultOk);
        assert_eq!(component.setActive(0), kResultOk);
        assert_eq!(component.terminate(), kResultOk);
    }
}

#[test]
fn editor_has_visible_fixed_size_and_rejects_invalid_scale() {
    let plugin = GlicolVst3::default();
    let editor = editor::GlicolEditor::new(plugin.params.clone());
    assert_eq!(editor.size(), (600, 800));
    assert!(!editor.set_scale_factor(f32::NAN));
    assert!(!editor.set_scale_factor(0.0));
    assert!(editor.set_scale_factor(1.5));
}
