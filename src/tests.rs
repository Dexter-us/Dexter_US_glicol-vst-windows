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
    for layout in GlicolVst3::AUDIO_IO_LAYOUTS {
        assert!(plugin.initialize(layout, &config, &mut context));
    }
    let mut unsupported = GlicolVst3::AUDIO_IO_LAYOUTS[0];
    unsupported.main_input_channels = NonZeroU32::new(3);
    assert!(!plugin.initialize(&unsupported, &config, &mut context));
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

    use vst3::Steinberg::Vst::SpeakerArr::{kMono, kStereo};
    // Exercise actual host bus negotiation and COM ABI for every layout.
    for (input_channels, output_channels) in [(2, 2), (1, 1), (1, 2)] {
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
            let mut input_arrangement = if input_channels == 1 { kMono } else { kStereo };
            let mut output_arrangement = if output_channels == 1 { kMono } else { kStereo };
            assert_eq!(
                processor.setBusArrangements(&mut input_arrangement, 1, &mut output_arrangement, 1),
                kResultOk
            );
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
                    numChannels: input_channels,
                    silenceFlags: 0,
                    __field0: AudioBusBuffers__type0 {
                        channelBuffers32: in_ptrs.as_mut_ptr(),
                    },
                };
                let mut output_bus = AudioBusBuffers {
                    numChannels: output_channels,
                    silenceFlags: 3,
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
                assert_eq!(
                    output_bus.silenceFlags, 0,
                    "Generated audio must not be marked silent"
                );
                assert_eq!(
                    processor.getTailSamples(),
                    u32::MAX,
                    "Host must keep generators alive"
                );
                for sample in 0..size {
                    let expected = if position < BLOCK_SIZE {
                        [0.0; 2]
                    } else {
                        [0.05, if input_channels == 1 { 0.05 } else { -0.025 }]
                    };
                    for channel in 0..output_channels as usize {
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
}

#[test]
fn editor_has_visible_fixed_size_and_rejects_invalid_scale() {
    let plugin = GlicolVst3::default();
    let editor = editor::GlicolEditor::new(plugin.params.clone());
    assert_eq!(editor.size(), (560, 420));
    assert!(!editor.set_scale_factor(f32::NAN));
    assert!(!editor.set_scale_factor(0.0));
    assert!(editor.set_scale_factor(1.5));
    assert!(editor.size().1 as f32 * 1.5 <= 600.0);
    assert!(editor.set_scale_factor(2.0));
    assert!(editor.size().1 as f32 * 2.0 <= 600.0);
}

#[test]
fn run_tone_on_silent_input_applies_without_waiting_for_a_bar() {
    let mut plugin = GlicolVst3::default();
    run_buffer(&mut plugin.audio, [vec![0.0; 1024], vec![0.0; 1024]]);
    plugin.params.submit(TEST_TONE.into()).unwrap();
    let output = run_buffer(&mut plugin.audio, [vec![0.0; 512], vec![0.0; 512]]);
    let peak = output[0].iter().fold(0.0_f32, |a, v| a.max(v.abs()));
    assert!(peak > 0.04 && peak <= 0.051);
    assert!(plugin.params.diagnostics.peaks().1 > 0.04);
    plugin.params.submit("o: sig 0;".into()).unwrap();
    let stopped = run_buffer(&mut plugin.audio, [vec![0.0; 512], vec![0.0; 512]]);
    assert!(stopped[0][BLOCK_SIZE..]
        .iter()
        .all(|sample| sample.abs() < 0.00001));
}

#[test]
fn mono_and_mono_to_stereo_preserve_audio() {
    let mut plugin = GlicolVst3::default();
    plugin.audio.set_input_channels(1);
    let mut mono = vec![0.5; 512];
    let mut buffer = Buffer::default();
    unsafe {
        buffer.set_slices(mono.len(), |slices| {
            slices.clear();
            slices.push(&mut mono);
        });
    }
    plugin.audio.process(&mut buffer);
    drop(buffer);
    assert!(mono[BLOCK_SIZE..]
        .iter()
        .all(|v| (*v - 0.05).abs() < 0.00001));
    plugin.audio.reset();
    let stereo = run_buffer(&mut plugin.audio, [vec![0.5; 512], vec![0.0; 512]]);
    for channel in stereo {
        assert!(channel[BLOCK_SIZE..]
            .iter()
            .all(|v| (*v - 0.05).abs() < 0.00001));
    }
}

#[test]
fn engine_errors_remain_visible_until_code_is_fixed() {
    let mut plugin = GlicolVst3::default();
    plugin.params.submit("o: ~missing;".into()).unwrap();
    run_buffer(&mut plugin.audio, [vec![0.0; 512], vec![0.0; 512]]);
    let error = plugin
        .params
        .diagnostics
        .error()
        .expect("Error must be visible");
    assert!(error.contains("missing"), "{error}");
    // A later successful audio block isn't a successful compile.
    run_buffer(&mut plugin.audio, [vec![0.0; 512], vec![0.0; 512]]);
    assert_eq!(
        plugin.params.diagnostics.error().as_deref(),
        Some(error.as_str())
    );
    plugin.params.submit("o: ~missing;".into()).unwrap();
    run_buffer(&mut plugin.audio, [vec![0.0; 512], vec![0.0; 512]]);
    assert!(plugin.params.diagnostics.error().is_some());
    plugin.params.submit(TEST_TONE.into()).unwrap();
    let output = run_buffer(&mut plugin.audio, [vec![0.0; 512], vec![0.0; 512]]);
    assert!(plugin.params.diagnostics.error().is_none());
    assert!(output[0].iter().any(|sample| sample.abs() > 0.04));
}

#[test]
fn editor_controls_and_scroll_clip_fit_short_windows() {
    let plugin = GlicolVst3::default();
    for height in [200.0, 300.0, 420.0] {
        let mut context = egui::CtxRef::default();
        let mut state = editor::EditorState::new(&plugin.params);
        state.code = "o: sin 440 >> mul 0.05;\n".repeat(200);
        context.begin_frame(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(560.0, height),
            )),
            ..Default::default()
        });
        let (buttons, code_clip) = editor::draw_editor(&context, &plugin.params, &mut state);
        let _ = context.end_frame();
        assert!(buttons.bottom() < height);
        assert!(code_clip.bottom() <= height + 1.0, "{code_clip:?}");
        assert!(code_clip.height() > 20.0, "{code_clip:?}");
    }
}

#[test]
fn default_program_must_not_report_a_syntax_error() {
    let mut plugin = GlicolVst3::default();
    run_buffer(&mut plugin.audio, [vec![0.5; 512], vec![0.5; 512]]);
    assert!(
        plugin.params.diagnostics.error().is_none(),
        "{:?}",
        plugin.params.diagnostics.error()
    );
}

#[test]
fn code_scroll_wheel_actually_moves_long_text() {
    let plugin = GlicolVst3::default();
    let mut ctx = egui::CtxRef::default();
    let mut state = editor::EditorState::new(&plugin.params);
    state.code = "o: sin 440 >> mul 0.05;\n".repeat(200);
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(560.0, 420.0));
    let mut viewport = screen;
    for frame in 0..3 {
        ctx.begin_frame(egui::RawInput {
            screen_rect: Some(screen),
            time: Some(frame as f64 / 60.0),
            events: if frame == 1 {
                vec![egui::Event::PointerMoved(viewport.center())]
            } else {
                vec![]
            },
            scroll_delta: if frame == 1 {
                egui::vec2(0.0, -150.0)
            } else {
                egui::Vec2::ZERO
            },
            ..Default::default()
        });
        let (_, clip) = editor::draw_editor(&ctx, &plugin.params, &mut state);
        viewport = clip;
        let _ = ctx.end_frame();
        if frame == 0 {
            assert!((state.code_rect.top() - viewport.top()).abs() < 5.0);
        } else if frame == 2 {
            assert!(
                state.code_rect.top() < viewport.top() - 100.0,
                "text top {}, viewport top {}",
                state.code_rect.top(),
                viewport.top()
            );
        }
    }
}

#[test]
fn dragging_code_scrollbar_actually_moves_long_text() {
    let plugin = GlicolVst3::default();
    let mut ctx = egui::CtxRef::default();
    let mut state = editor::EditorState::new(&plugin.params);
    state.code = "o: sin 440 >> mul 0.05;\n".repeat(200);
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(560.0, 420.0));
    let mut viewport = screen;
    for frame in 0..5 {
        let top = egui::pos2(screen.right() - 14.0, viewport.top() + 10.0);
        let bottom = egui::pos2(top.x, viewport.center().y);
        let events = match frame {
            1 => vec![
                egui::Event::PointerMoved(top),
                egui::Event::PointerButton {
                    pos: top,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
            2 => vec![egui::Event::PointerMoved(bottom)],
            3 => vec![egui::Event::PointerButton {
                pos: bottom,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
            _ => vec![],
        };
        ctx.begin_frame(egui::RawInput {
            // Allow the auto-show scrollbar's animation to finish before dragging.
            screen_rect: Some(screen),
            time: Some(frame as f64),
            events,
            ..Default::default()
        });
        let (_, clip) = editor::draw_editor(&ctx, &plugin.params, &mut state);
        viewport = clip;
        let _ = ctx.end_frame();
        if frame == 4 {
            assert!(
                state.code_rect.top() < viewport.top() - 100.0,
                "text {:?}, viewport {:?}",
                state.code_rect,
                viewport
            );
        }
    }
}

#[test]
fn short_program_does_not_create_empty_scroll_overflow() {
    let plugin = GlicolVst3::default();
    let mut ctx = egui::CtxRef::default();
    let mut state = editor::EditorState::new(&plugin.params);
    ctx.begin_frame(egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(560.0, 420.0),
        )),
        ..Default::default()
    });
    let (_, clip) = editor::draw_editor(&ctx, &plugin.params, &mut state);
    let _ = ctx.end_frame();
    assert!(
        state.code_rect.bottom() <= clip.bottom(),
        "Short programs must not reserve a tall blank scrollable area"
    );
}

#[test]
fn prose_tone_label_reproduces_error_and_standalone_programs_fix_it() {
    let mut plugin = GlicolVst3::default();
    plugin
        .params
        .submit("o: ~input >> mul 0.1;\n\n Tone test: o: sin 440 >> mul 0.1;".into())
        .unwrap();
    run_buffer(&mut plugin.audio, [vec![0.0; 512], vec![0.0; 512]]);
    let error = plugin.params.diagnostics.error().unwrap();
    assert!(
        error.contains("line[3]") && error.contains("col[2]"),
        "{error}"
    );
    plugin.params.submit(TEST_TONE.into()).unwrap();
    let output = run_buffer(&mut plugin.audio, [vec![0.0; 512], vec![0.0; 512]]);
    assert!(plugin.params.diagnostics.error().is_none());
    assert!(output[0].iter().any(|v| v.abs() > 0.04));
    plugin.params.submit("o: ~input;".into()).unwrap();
    let output = run_buffer(&mut plugin.audio, [vec![0.25; 512], vec![0.25; 512]]);
    assert!(plugin.params.diagnostics.error().is_none());
    assert!(output[0][BLOCK_SIZE..]
        .iter()
        .all(|v| (*v - 0.25).abs() < 0.00001));
}

#[test]
fn save_and_load_buttons_queue_actions_without_submitting_audio() {
    use crate::program_files::FileAction;
    let plugin = GlicolVst3::default();
    for action in [FileAction::Save, FileAction::Load] {
        let mut ctx = egui::CtxRef::default();
        let mut state = editor::EditorState::new(&plugin.params);
        state.code = "o: speed 2.0 >> seq  55 60 _90 _ 48__90 >> mul 0.8".into();
        let mut button = egui::Pos2::ZERO;
        for frame in 0..3 {
            let events = if frame == 0 {
                vec![]
            } else {
                vec![
                    egui::Event::PointerMoved(button),
                    egui::Event::PointerButton {
                        pos: button,
                        button: egui::PointerButton::Primary,
                        pressed: frame == 1,
                        modifiers: Default::default(),
                    },
                ]
            };
            ctx.begin_frame(egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(560.0, 420.0),
                )),
                time: Some(frame as f64 / 60.0),
                events,
                ..Default::default()
            });
            editor::draw_editor(&ctx, &plugin.params, &mut state);
            button = match action {
                FileAction::Save => state.save_rect.center(),
                FileAction::Load => state.load_rect.center(),
            };
            let _ = ctx.end_frame();
        }
        assert_eq!(state.file_request, Some(action));
        assert_eq!(*plugin.params.code.lock().unwrap(), DEFAULT_CODE);
    }
}

#[test]
fn loaded_text_stays_a_draft_until_run_and_keeps_restore_text() {
    let plugin = GlicolVst3::default();
    let mut state = editor::EditorState::new(&plugin.params);
    let text = "o: speed 2.0 >> seq  55 60 _90 _ 48__90 >> mul 0.8";
    state.apply_file("program.txt".into(), text.into());
    assert_eq!(state.code, text);
    assert_eq!(state.file_snapshot.as_deref(), Some(text));
    assert_eq!(
        state.file_path.as_deref(),
        Some(std::path::Path::new("program.txt"))
    );
    assert!(state.file_status.as_deref().unwrap().contains("click Run"));
    assert_eq!(*plugin.params.code.lock().unwrap(), DEFAULT_CODE);
}
