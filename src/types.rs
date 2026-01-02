// types.rs - Struktury danych i enumy
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Language {
    En,
    Pl,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TrackType {
    #[default]
    Both,
    Video,
    Audio,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Clip {
    pub start: f32,
    pub end: f32,
    #[serde(default)]
    pub fade_in: f32,
    #[serde(default)]
    pub fade_out: f32,
    #[serde(default = "default_true")]
    pub linked: bool,
    #[serde(default = "default_true")]
    pub video_enabled: bool,
    #[serde(default = "default_true")]
    pub audio_enabled: bool,
}

#[derive(Serialize, Deserialize)]
pub struct ProjectData {
    pub input_path: String,
    pub output_path: String,
    pub clips: Vec<Clip>,
    pub duration: f32,
    pub playhead: f32,
}

#[derive(Clone, Copy)]
pub enum FadeKind {
    In,
    Out,
}

#[derive(Clone, Copy)]
pub struct FadeDrag {
    pub clip_idx: usize,
    pub kind: FadeKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Hand,
    Scissors,
}
