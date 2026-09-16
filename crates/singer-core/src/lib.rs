use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PlaybackMode {
    Original,
    Accompaniment,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct KaraokeCue {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct KaraokeTrack {
    pub mode: String,
    pub version: String,
    pub source_type: String,
    pub source_file: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_page: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub license: String,
    pub language: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub lyrics_language: String,
    #[serde(default, rename = "lyrics_offset_seconds")]
    pub lyrics_offset: f64,
    #[serde(default, rename = "duration_seconds")]
    pub duration: f64,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub lyrics: Vec<KaraokeCue>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub synced_at_shanghai: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct KaraokeSong {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub year: i32,
    #[serde(default)]
    pub track_no: i32,
    #[serde(default)]
    pub version: String,
    pub language: String,
    #[serde(default)]
    pub lyrics_language: String,
    #[serde(default)]
    pub lyrics_version: String,
    #[serde(default)]
    pub lyrics: Vec<KaraokeCue>,
    #[serde(default)]
    pub tracks: BTreeMap<String, KaraokeTrack>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct KaraokeAlbum {
    pub id: String,
    pub title: String,
    pub year: i32,
    pub language: String,
    pub order: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct KaraokeCatalog {
    pub provider: String,
    pub language: String,
    #[serde(default)]
    pub updated_at_shanghai: String,
    #[serde(default)]
    pub albums: Vec<KaraokeAlbum>,
    #[serde(default)]
    pub songs: Vec<KaraokeSong>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct KaraokeRecording {
    pub id: String,
    pub file: String,
    pub song_id: String,
    pub song_title: String,
    pub mode: String,
    pub duration_seconds: i64,
    pub created_at_shanghai: String,
    pub started_at_shanghai: String,
    pub bytes: u64,
    pub url: String,
}
