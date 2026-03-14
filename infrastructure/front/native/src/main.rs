mod app;
mod socket;
mod types;
mod video;

use clap::Parser;
use std::sync::mpsc;

#[derive(Parser)]
#[command(name = "mediachat-native", about = "MediaChat native overlay — no webview")]
struct Args {
    /// Room key to join (same as the URL fragment in the web viewer)
    #[arg(short, long, default_value = "default")]
    room: String,

    /// MediaChat backend URL
    #[arg(short, long, env = "MEDIACHAT_SERVER", default_value = "http://localhost:3000")]
    server: String,
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = Args::parse();

    // Initialise FFmpeg once (idempotent, safe to call before any threads spawn)
    ffmpeg_next::init().expect("FFmpeg initialisation failed — are the system libraries installed?");

    // ── event channel: socket → app ──────────────────────────────────────────
    let (event_tx, event_rx) = mpsc::channel::<types::AppEvent>();

    // ── Socket.IO in a dedicated OS thread with its own Tokio runtime ────────
    {
        let tx = event_tx.clone();
        let server = args.server.clone();
        let room = args.room.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            if let Err(e) = rt.block_on(socket::run_socket(server, room, tx)) {
                log::error!("Socket.IO thread exited with error: {e}");
            }
        });
    }

    // ── egui/eframe native window ────────────────────────────────────────────
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            // Frameless transparent overlay, always on top
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_fullscreen(true)
            // Clicks pass through to the window below
            .with_mouse_passthrough(true)
            // Don't steal keyboard focus from the streamer's game/app
            .with_active(false),
        // wgpu backend — GPU-accelerated, works on Vulkan / Metal / DX12
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "MediaChat",
        options,
        Box::new(move |_cc| Ok(Box::new(app::App::new(event_tx, event_rx)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))
}
