mod app;
mod media;
mod socket;
mod types;
mod ui;
mod video;
use clap::Parser;
use std::sync::mpsc;
use types::new_waker;
mod windows;

#[derive(Parser)]
#[command(
    name = "mediachat-native",
    about = "MediaChat native overlay — no webview"
)]
struct Args {
    /// Room key to join (same as the URL fragment in the web viewer)
    #[arg(short, long, default_value = "default")]
    room: String,

    /// MediaChat backend URL
    #[arg(
        short,
        long,
        env = "MEDIACHAT_SERVER",
        default_value = "http://localhost:3000"
    )]
    server: String,
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = Args::parse();

    // ── event channel: socket → app ──────────────────────────────────────────
    let (event_tx, event_rx) = mpsc::channel::<types::AppEvent>();

    // ── waker: set once egui context is ready, then used by all bg threads ──
    let waker = new_waker();

    // ── Socket.IO in a dedicated OS thread with its own Tokio runtime ────────
    {
        let tx = event_tx.clone();
        let waker = waker.clone();
        let server = args.server;
        let room = args.room;
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            if let Err(e) = rt.block_on(socket::run_socket(server, room, tx, waker)) {
                log::error!("Socket.IO thread exited with error: {e}");
            }
        });
    }

    // ── egui/eframe native window ────────────────────────────────────────────
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_always_on_top()
            // with_transparent(true) intentionally removed: on NVIDIA the glow renderer
            // outputs alpha=0 for all pixels, making everything invisible. Transparency
            // is handled instead via Win32 SetLayeredWindowAttributes(LWA_COLORKEY).
            .with_mouse_passthrough(true)
            .with_active(false)
            .with_fullscreen(false)
            .with_decorations(false)
            .with_taskbar(false),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "MediaChat",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, event_tx, event_rx, waker)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))
}
