//! Adapt arbitrary host callback sizes to the engine's fixed block size.
//! The output is delayed by one engine block, independent of callback size.

pub const BLOCK_SIZE: usize = 128;

pub struct BlockAdapter {
    input: [[f32; BLOCK_SIZE]; 2],
    output: [[f32; BLOCK_SIZE]; 2],
    input_pos: usize,
    output_pos: usize,
}

impl Default for BlockAdapter {
    fn default() -> Self {
        Self {
            input: [[0.0; BLOCK_SIZE]; 2],
            output: [[0.0; BLOCK_SIZE]; 2],
            input_pos: 0,
            output_pos: BLOCK_SIZE,
        }
    }
}

impl BlockAdapter {
    pub fn process_frame<F>(&mut self, input: [f32; 2], render: F) -> [f32; 2]
    where
        F: FnOnce(&[f32; BLOCK_SIZE], &[f32; BLOCK_SIZE]) -> [[f32; BLOCK_SIZE]; 2],
    {
        let output = if self.output_pos < BLOCK_SIZE {
            let frame = [
                self.output[0][self.output_pos],
                self.output[1][self.output_pos],
            ];
            self.output_pos += 1;
            frame
        } else {
            [0.0; 2]
        };

        self.input[0][self.input_pos] = input[0];
        self.input[1][self.input_pos] = input[1];
        self.input_pos += 1;

        if self.input_pos == BLOCK_SIZE {
            self.output = render(&self.input[0], &self.input[1]);
            self.input_pos = 0;
            self.output_pos = 0;
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_callbacks(callbacks: &[usize]) {
        let mut adapter = BlockAdapter::default();
        let mut rendered_blocks = 0;
        let mut position = 0;
        // Several cycles exercise boundaries spanning more than one callback.
        for _ in 0..8 {
            for &size in callbacks {
                for _ in 0..size {
                    let frame = adapter.process_frame(
                        [position as f32 + 1.0, -(position as f32 + 1.0)],
                        |left, right| {
                            rendered_blocks += 1;
                            [*left, *right]
                        },
                    );
                    let expected = if position < BLOCK_SIZE {
                        [0.0; 2]
                    } else {
                        let value = (position - BLOCK_SIZE) as f32 + 1.0;
                        [value, -value]
                    };
                    assert_eq!(frame, expected, "sample {position}, callback size {size}");
                    position += 1;
                }
            }
        }
        assert_eq!(rendered_blocks, position / BLOCK_SIZE);
    }

    #[test]
    fn sub_block_callbacks_produce_audio() {
        for size in [1, 16, 32, 64, 127] {
            check_callbacks(&[size]);
        }
    }

    #[test]
    fn aligned_callbacks_preserve_stereo() {
        for size in [128, 256, 512, 1024] {
            check_callbacks(&[size]);
        }
    }

    #[test]
    fn irregular_callbacks_do_not_drop_remainders() {
        check_callbacks(&[0, 1, 63, 129, 192, 257, 1000, 17]);
    }

    #[test]
    fn reset_discards_old_input_and_output() {
        let mut adapter = BlockAdapter::default();
        for _ in 0..193 {
            adapter.process_frame([1.0; 2], |left, right| [*left, *right]);
        }
        adapter = BlockAdapter::default();
        for _ in 0..BLOCK_SIZE {
            assert_eq!(
                adapter.process_frame([2.0; 2], |left, right| [*left, *right]),
                [0.0; 2]
            );
        }
        assert_eq!(
            adapter.process_frame([2.0; 2], |left, right| [*left, *right]),
            [2.0; 2]
        );
    }
}
