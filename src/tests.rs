use super::*;
use vst::host::HostBuffer;

#[test]
fn effect_writes_every_sample_at_arbitrary_callback_sizes() {
    let mut plugin = GlicolVSTPlugin::default();
    assert_eq!(plugin.get_info().initial_delay, BLOCK_SIZE as i32);
    assert_eq!(plugin.get_info().vendor, "Dexter U.S.");
    plugin.resume();
    let mut host = HostBuffer::new(2, 2);
    let mut position = 0;
    for _ in 0..4 {
        for size in [32, 64, 127, 128, 192, 257, 512] {
            let input = [vec![0.5; size], vec![-0.25; size]];
            let mut output = [vec![f32::NAN; size], vec![f32::NAN; size]];
            plugin.process(&mut host.bind(&input, &mut output));
            for sample in 0..size {
                let expected = if position < BLOCK_SIZE {
                    [0.0, 0.0]
                } else {
                    [0.05, -0.025]
                };
                for channel in 0..2 {
                    assert!(
                        (output[channel][sample] - expected[channel]).abs() < 0.00001,
                        "sample {position}, channel {channel}: {}",
                        output[channel][sample]
                    );
                }
                position += 1;
            }
        }
    }
}

#[test]
fn owned_code_update_generates_tone_without_audio_input() {
    let mut plugin = GlicolVSTPlugin::default();
    let mut editor_text = "o: sin 440 >> mul 0.1;".to_owned();
    plugin
        .params
        .updates
        .lock()
        .unwrap()
        .push(editor_text.clone())
        .unwrap();
    // Reallocating or closing the editor no longer invalidates the update.
    editor_text.clear();
    editor_text.push_str(&"x".repeat(10000));
    let mut host = HostBuffer::new(2, 2);
    let mut peak = 0.0_f32;
    for _ in 0..64 {
        let input = [vec![0.0; 64], vec![0.0; 64]];
        let mut output = [vec![f32::NAN; 64], vec![f32::NAN; 64]];
        plugin.process(&mut host.bind(&input, &mut output));
        for sample in output.iter().flatten() {
            assert!(sample.is_finite());
            peak = peak.max(sample.abs());
        }
    }
    assert!(peak > 0.09 && peak <= 0.101, "tone peak: {}", peak);
}

#[test]
fn sample_rate_is_applied_to_tone_generation() {
    let mut plugin = GlicolVSTPlugin::default();
    *plugin.params.code.lock().unwrap() = "o: sin 440 >> mul 0.1;".to_owned();
    plugin.set_sample_rate(48000.0);
    let mut host = HostBuffer::new(2, 2);
    let input = [vec![0.0; 48000 + BLOCK_SIZE], vec![0.0; 48000 + BLOCK_SIZE]];
    let mut output = [
        vec![f32::NAN; input[0].len()],
        vec![f32::NAN; input[0].len()],
    ];
    plugin.process(&mut host.bind(&input, &mut output));
    let crossings = output[0][BLOCK_SIZE..]
        .windows(2)
        .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
        .count();
    assert!(
        (439..=441).contains(&crossings),
        "tone crossings: {}",
        crossings
    );
}
