use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlaybackMode {
    Original,
    Accompaniment,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LyricCue {
    pub start_ms: u32,
    pub end_ms: u32,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct KaraokeSong {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub language: String,
    pub lyrics: Vec<LyricCue>,
}
