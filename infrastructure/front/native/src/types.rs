use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Author {
    pub id: String,
    pub name: String,
    pub image: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Video,
    Image,
    Sound,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Media {
    pub id: String,
    pub url: String,
    #[serde(rename = "type")]
    pub media_type: MediaType,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct TextOptions {
    #[serde(rename = "positionX")]
    pub position_x: Option<String>,
    #[serde(rename = "positionY")]
    pub position_y: Option<String>,
    pub color: Option<String>,
    #[serde(rename = "fontSize")]
    pub font_size: Option<f32>,
    #[serde(rename = "fontFamily")]
    pub font_family: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct FileOptions {
    #[serde(rename = "positionX")]
    pub position_x: Option<String>,
    #[serde(rename = "positionY")]
    pub position_y: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct MediachatOptions {
    pub file: Option<FileOptions>,
    pub text: Option<TextOptions>,
    #[serde(rename = "hideAuthor")]
    pub hide_author: Option<bool>,
    pub target: Option<String>,
    pub target_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MediaChat {
    pub id: String,
    pub author: Author,
    pub duration: Option<f64>,
    pub message: Option<String>,
    pub media: Option<Media>,
    pub options: Option<MediachatOptions>,
}

// ---------- internal app messages ----------

pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    /// RGBA packed bytes, length = width * height * 4
    pub data: Vec<u8>,
    /// Presentation timestamp in seconds
    pub pts_secs: f64,
}

pub enum AppEvent {
    // Socket.IO events
    NewMediaChat(MediaChat),
    Flush,
    Skip,

    // Asset download results
    AvatarLoaded(Vec<u8>),
    MediaImageLoaded(Vec<u8>),

    /// Video decoder is ready.
    /// `frame_rx`  — receive decoded RGBA frames
    /// `audio_path` — temp file path to pass to ffplay for audio (None if no audio stream)
    VideoReady {
        frame_rx: std::sync::mpsc::Receiver<VideoFrame>,
        audio_path: Option<String>,
    },

    /// All video frames have been sent (channel may still have buffered frames)
    VideoEnded,
}
