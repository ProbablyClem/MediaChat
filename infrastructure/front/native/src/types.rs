use std::sync::{mpsc::Receiver, Arc, OnceLock};

use crate::{media::MediaChat, video::VideoFrame};

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
        frame_rx: Receiver<VideoFrame>,
        audio_path: Option<String>,
    },

    /// All video frames have been sent (channel may still have buffered frames)
    VideoEnded,
}

// ─── egui wakeup helper ────────────────────────────────────────────────────
/// A cheaply-cloneable handle that lets background threads request an egui repaint.
/// The inner OnceLock is set once, in App::new, after the egui context is ready.
pub type CtxWaker = Arc<OnceLock<egui::Context>>;

pub fn new_waker() -> CtxWaker {
    Arc::new(OnceLock::new())
}

pub fn wake(w: &CtxWaker) {
    if let Some(ctx) = w.get() {
        ctx.request_repaint();
    }
}
