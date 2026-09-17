#![feature(min_adt_const_params)]

use std::marker::ConstParamTy;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ConstParamTy)]
pub enum RouteKind {
    MicOnly,
    ProgramMix,
}

pub struct AudioGraph<const ROUTE: RouteKind>;

impl AudioGraph<{ RouteKind::MicOnly }> {
    pub fn attach_recorder(&self) -> Recorder {
        Recorder
    }
}

impl AudioGraph<{ RouteKind::ProgramMix }> {
    pub fn monitor_only(&self) {}
}

pub struct Recorder;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_capability_is_a_const_type_parameter() {
        let graph = AudioGraph::<{ RouteKind::MicOnly }>;
        let _recorder = graph.attach_recorder();
    }
}
