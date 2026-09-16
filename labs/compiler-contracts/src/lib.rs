#![feature(negative_impls)]
#![feature(negative_bounds)]

pub trait ProgramAudioPresent {}

pub struct MicOnlyBus;
pub struct MixedBus;

impl !ProgramAudioPresent for MicOnlyBus {}
impl ProgramAudioPresent for MixedBus {}

pub fn require_program_audio_absent<T>()
where
    T: !ProgramAudioPresent,
{
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mic_only_satisfies_negative_contract() {
        require_program_audio_absent::<MicOnlyBus>();
    }
}
