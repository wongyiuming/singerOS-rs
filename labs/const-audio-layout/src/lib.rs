#![allow(incomplete_features)]
#![feature(min_generic_const_args)]

const fn sample_count(frames: usize, channels: usize) -> usize {
    frames * channels
}

pub struct AudioBlock<const FRAMES: usize, const CHANNELS: usize> {
    samples: [
        f32;
        core::direct_const_arg!(const { sample_count(FRAMES, CHANNELS) })
    ],
}

impl<const FRAMES: usize, const CHANNELS: usize> AudioBlock<FRAMES, CHANNELS> {
    pub fn silence() -> Self {
        Self {
            samples: [
                0.0;
                core::direct_const_arg!(const { sample_count(FRAMES, CHANNELS) })
            ],
        }
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_block_has_type_level_sample_count() {
        let block = AudioBlock::<128, 2>::silence();
        assert_eq!(block.samples().len(), 256);
    }
}
