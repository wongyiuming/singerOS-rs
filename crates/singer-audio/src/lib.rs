use std::marker::PhantomData;

pub struct MicOnly;
pub struct ProgramMix;
pub struct Idle;
pub struct Running;

pub trait RecordableRoute {}
impl RecordableRoute for MicOnly {}

pub struct AudioGraph<Route, State> {
    record_gain: f32,
    _route: PhantomData<Route>,
    _state: PhantomData<State>,
}

impl AudioGraph<MicOnly, Idle> {
    pub fn microphone() -> Self {
        Self { record_gain: 1.0, _route: PhantomData, _state: PhantomData }
    }
}

impl AudioGraph<ProgramMix, Idle> {
    pub fn program_mix() -> Self {
        Self { record_gain: 1.0, _route: PhantomData, _state: PhantomData }
    }
}

impl<Route> AudioGraph<Route, Idle> {
    pub fn start(self) -> AudioGraph<Route, Running> {
        AudioGraph { record_gain: self.record_gain, _route: PhantomData, _state: PhantomData }
    }
}

impl<State> AudioGraph<MicOnly, State> {
    pub fn set_record_gain(&mut self, gain: f32) {
        self.record_gain = gain.clamp(0.0, 3.0);
    }
}

pub fn attach_recorder<Route: RecordableRoute, State>(_graph: &AudioGraph<Route, State>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mic_only_route_is_recordable() {
        let graph = AudioGraph::<MicOnly, Idle>::microphone().start();
        attach_recorder(&graph);
    }
}
