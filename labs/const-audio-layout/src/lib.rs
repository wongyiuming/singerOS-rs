#![allow(incomplete_features)]
#![feature(generic_const_args)]
#![feature(generic_const_items)]
#![feature(min_generic_const_args)]

pub type const SAMPLE_COUNT<const FRAMES: usize, const CHANNELS: usize>: usize =
    const { FRAMES * CHANNELS };

pub struct AudioBlock<const FRAMES: usize, const CHANNELS: usize> {
    samples: [f32; SAMPLE_COUNT::<FRAMES, CHANNELS>],
}

impl<const FRAMES: usize, const CHANNELS: usize> AudioBlock<FRAMES, CHANNELS> {
    pub fn silence() -> Self {
        Self {
            samples: [0.0; SAMPLE_COUNT::<FRAMES, CHANNELS>],
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
