use std::{
    collections::VecDeque,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc::{Receiver, Sender},
    time::Instant,
};

use egui::{
    Align2, Color32, ColorImage, FontId, Pos2, Rect, TextureHandle, TextureOptions, Vec2,
};

use crate::{
    types::{AppEvent, MediaChat, MediaType, VideoFrame},
    video::spawn_video_decoder,
};

// ─────────────────────────────────────────────────────────────────────────────
//  App state
// ─────────────────────────────────────────────────────────────────────────────

pub struct App {
    event_tx: Sender<AppEvent>,
    event_rx: Receiver<AppEvent>,

    /// FIFO — index 0 is the currently displayed item
    queue: VecDeque<MediaChat>,
    current: Option<ActiveMedia>,

    /// ffplay/paplay child process for the current audio (killed on advance)
    audio_child: Option<Child>,

    http: reqwest::blocking::Client,

    /// Whether Win32 overlay setup has been done (runs once after window is ready)
    win32_initialized: bool,
}

struct ActiveMedia {
    chat: MediaChat,

    avatar_tex: Option<TextureHandle>,
    media_tex: Option<TextureHandle>,  // image-type media
    frame_tex: Option<TextureHandle>,  // current video frame

    /// Bounded receiver from the video decoder thread
    frame_rx: Option<Receiver<VideoFrame>>,
    /// Decoded frames waiting to be displayed at the right PTS
    pending_frames: VecDeque<VideoFrame>,
    video_ended: bool,
    /// Wall-clock instant when the first frame was received (video clock origin)
    video_clock: Option<Instant>,

    /// Wall-clock instant this item started displaying
    started_at: Instant,

    /// Temp file to clean up after the video finishes
    temp_path: Option<PathBuf>,
}

impl ActiveMedia {
    fn new(chat: MediaChat) -> Self {
        Self {
            chat,
            avatar_tex: None,
            media_tex: None,
            frame_tex: None,
            frame_rx: None,
            pending_frames: VecDeque::new(),
            video_ended: false,
            video_clock: None,
            started_at: Instant::now(),
            temp_path: None,
        }
    }

    fn is_video(&self) -> bool {
        self.chat
            .media
            .as_ref()
            .map(|m| m.media_type == MediaType::Video)
            .unwrap_or(false)
    }

    fn should_advance(&self) -> bool {
        if self.is_video() {
            self.video_ended && self.pending_frames.is_empty()
        } else {
            let dur = self.chat.duration.unwrap_or(5.0);
            self.started_at.elapsed().as_secs_f64() >= dur
        }
    }
}

impl Drop for ActiveMedia {
    fn drop(&mut self) {
        // Remove the downloaded video temp file when this item is done
        if let Some(ref path) = self.temp_path {
            let _ = std::fs::remove_file(path);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  App impl
// ─────────────────────────────────────────────────────────────────────────────

impl App {
    pub fn new(
        _cc: &eframe::CreationContext,
        event_tx: Sender<AppEvent>,
        event_rx: Receiver<AppEvent>,
    ) -> Self {
        Self {
            event_tx,
            event_rx,
            queue: VecDeque::new(),
            current: None,
            audio_child: None,
            http: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap(),
            win32_initialized: false,
        }
    }

    // ── item lifecycle ────────────────────────────────────────────────────────

    fn start_item(&mut self, chat: MediaChat) {
        self.kill_audio();
        self.current = None; // triggers ActiveMedia::drop → temp file cleanup

        let active = ActiveMedia::new(chat.clone());

        if let Some(ref url) = chat.author.image {
            self.download_in_bg(url.clone(), AppEvent::AvatarLoaded);
        }

        if let Some(ref media) = chat.media {
            match media.media_type {
                MediaType::Image => {
                    self.download_in_bg(media.url.clone(), AppEvent::MediaImageLoaded);
                }
                MediaType::Video => {
                    spawn_video_decoder(media.url.clone(), self.event_tx.clone());
                }
                MediaType::Sound => {
                    // ffplay handles HTTP URLs directly — no download needed
                    self.play_audio_url(&media.url);
                }
            }
        }

        self.current = Some(active);
    }

    fn advance(&mut self) {
        self.kill_audio();
        self.current = None; // triggers drop
        if let Some(next) = self.queue.pop_front() {
            self.start_item(next);
        }
    }

    // ── audio via ffplay subprocess ──────────────────────────────────────────

    fn kill_audio(&mut self) {
        if let Some(mut child) = self.audio_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Play a URL (http/file) through ffplay in the background.
    fn play_audio_url(&mut self, url: &str) {
        match Command::new("ffplay")
            .args(["-nodisp", "-autoexit", "-loglevel", "quiet", url])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => self.audio_child = Some(child),
            Err(e) => log::warn!("Failed to spawn ffplay: {e}"),
        }
    }

    /// Play a local file path through ffplay.
    fn play_audio_file(&mut self, path: &str) {
        self.play_audio_url(path);
    }

    // ── background HTTP download ─────────────────────────────────────────────

    fn download_in_bg<F>(&self, url: String, make_event: F)
    where
        F: Fn(Vec<u8>) -> AppEvent + Send + 'static,
    {
        let http = self.http.clone();
        let tx = self.event_tx.clone();
        std::thread::spawn(move || {
            match http.get(&url).send().and_then(|r| r.bytes()) {
                Ok(bytes) => {
                    let _ = tx.send(make_event(bytes.to_vec()));
                }
                Err(e) => log::warn!("Download failed for {url}: {e}"),
            }
        });
    }

    // ── event processing ─────────────────────────────────────────────────────

    fn process_events(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.event_rx.try_recv() {
            match ev {
                AppEvent::NewMediaChat(mc) => {
                    if self.current.is_none() {
                        self.start_item(mc);
                    } else {
                        self.queue.push_back(mc);
                    }
                }

                AppEvent::Flush => {
                    self.queue.clear();
                    self.kill_audio();
                    self.current = None;
                }

                AppEvent::Skip => self.advance(),

                AppEvent::AvatarLoaded(data) => {
                    if let Some(active) = &mut self.current {
                        if let Some(ci) = decode_circular(&data) {
                            active.avatar_tex = Some(ctx.load_texture(
                                "avatar",
                                ci,
                                TextureOptions::LINEAR,
                            ));
                        }
                    }
                }

                AppEvent::MediaImageLoaded(data) => {
                    if let Some(active) = &mut self.current {
                        if let Some(ci) = decode_image(&data) {
                            active.media_tex = Some(ctx.load_texture(
                                "media",
                                ci,
                                TextureOptions::LINEAR,
                            ));
                        }
                    }
                }

                AppEvent::VideoReady { frame_rx, audio_path } => {
                    if let Some(active) = &mut self.current {
                        active.frame_rx = Some(frame_rx);

                        // Start audio for this video
                        if let Some(ref path) = audio_path {
                            self.play_audio_file(path);
                            // Also record the path for cleanup
                            if let Some(ref mut a) = self.current {
                                a.temp_path = Some(PathBuf::from(path));
                            }
                        }
                    }
                }

                AppEvent::VideoEnded => {
                    if let Some(active) = &mut self.current {
                        active.video_ended = true;
                    }
                }
            }
        }
    }

    // ── video frame advancement ───────────────────────────────────────────────

    fn update_video_frame(&mut self, ctx: &egui::Context) {
        let active = match &mut self.current {
            Some(a) if a.is_video() => a,
            _ => return,
        };

        // Pull newly decoded frames from the decoder into our local queue
        if let Some(ref rx) = active.frame_rx {
            while let Ok(frame) = rx.try_recv() {
                if active.video_clock.is_none() {
                    active.video_clock = Some(Instant::now());
                }
                active.pending_frames.push_back(frame);
            }
        }

        let elapsed = match active.video_clock {
            Some(t) => t.elapsed().as_secs_f64(),
            None => return,
        };

        // Discard frames whose PTS has passed, keep the most recent one
        let mut last: Option<VideoFrame> = None;
        while active
            .pending_frames
            .front()
            .map(|f| f.pts_secs <= elapsed)
            .unwrap_or(false)
        {
            last = active.pending_frames.pop_front();
        }

        if let Some(frame) = last {
            let ci = ColorImage::from_rgba_unmultiplied(
                [frame.width as usize, frame.height as usize],
                &frame.data,
            );
            active.frame_tex = Some(ctx.load_texture("vframe", ci, TextureOptions::LINEAR));
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  eframe::App — render loop
// ─────────────────────────────────────────────────────────────────────────────

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // One-time Win32 overlay setup: color-key transparency + remove decorations
        if !self.win32_initialized {
            self.win32_initialized = true;
            #[cfg(windows)]
            unsafe { win32_setup_overlay(); }
        }

        // Color key: DWM makes RGB(1,0,1) transparent at compositor level.
        // with_transparent(true) is NOT used — on NVIDIA the glow renderer outputs
        // alpha=0 for all pixels, making per-pixel alpha compositing invisible.
        let key = Color32::from_rgb(1, 0, 1);
        ctx.set_visuals(egui::Visuals {
            window_fill: key,
            panel_fill: key,
            ..egui::Visuals::dark()
        });

        self.process_events(ctx);
        self.update_video_frame(ctx);

        if self.current.as_ref().map(|a| a.should_advance()).unwrap_or(false) {
            self.advance();
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(16));

        let Some(active) = &self.current else {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(key))
                .show(ctx, |ui| {
                    let r = ui.max_rect();
                    let center = egui::Pos2::new(r.right() - 30.0, r.bottom() - 30.0);
                    ui.painter().circle_filled(center, 18.0, Color32::from_rgb(30, 30, 30));
                    ui.painter().circle_filled(center, 14.0, Color32::from_rgb(50, 220, 80));
                });
            return;
        };

        let chat = active.chat.clone();
        let avatar_tex = active.avatar_tex.clone();
        let media_tex = active.frame_tex.as_ref().or(active.media_tex.as_ref()).cloned();
        let time = ctx.input(|i| i.time);

        let screen = ctx.screen_rect();
        let w = screen.width();
        let h = screen.height();

        let row_top = h / 6.0;
        let row_mid = h * 4.0 / 6.0;
        let row_bot = h / 6.0;

        let hide_author = chat.options.as_ref().and_then(|o| o.hide_author).unwrap_or(false);

        let text_opts = chat.options.as_ref().and_then(|o| o.text.as_ref());
        let text_color = text_opts
            .and_then(|t| parse_color(t.color.as_deref()))
            .unwrap_or(Color32::WHITE);
        let text_size = text_opts.and_then(|t| t.font_size).unwrap_or(36.0);

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(key))
            .show(ctx, |ui| {
                let p = ui.painter();

                if !hide_author {
                    let float_y = screen.top()
                        + row_top / 2.0
                        + (time * std::f64::consts::TAU / 4.0).sin() as f32 * 8.0;
                    let cx = w / 2.0;
                    if let Some(ref tex) = avatar_tex {
                        let sz = 72.0_f32;
                        let rect = Rect::from_center_size(Pos2::new(cx, float_y), Vec2::splat(sz));
                        p.image(
                            tex.id(),
                            rect,
                            egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                            Color32::WHITE,
                        );
                        outlined_text(
                            p,
                            &chat.author.name.to_uppercase(),
                            Pos2::new(cx, float_y + sz / 2.0 + 10.0),
                            FontId::proportional(18.0),
                            Color32::WHITE,
                            Color32::BLACK,
                        );
                    }
                }

                let mid = Rect::from_min_size(
                    Pos2::new(screen.left(), screen.top() + row_top),
                    Vec2::new(w, row_mid),
                );
                if let Some(ref tex) = media_tex {
                    let ts = tex.size_vec2();
                    let scale = (mid.width() / ts.x).min(mid.height() / ts.y);
                    let disp = Rect::from_center_size(mid.center(), ts * scale);
                    p.image(
                        tex.id(),
                        disp,
                        egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }

                if let Some(ref msg) = chat.message {
                    if !msg.is_empty() {
                        let msg_y = screen.top() + row_top + row_mid + row_bot / 2.0;
                        outlined_text(
                            p,
                            &msg.to_uppercase(),
                            Pos2::new(w / 2.0, msg_y),
                            FontId::proportional(text_size),
                            text_color,
                            Color32::BLACK,
                        );
                    }
                }
            });
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Matches the color key: RGB(1,0,1) in ~linear space
        [1.0 / 255.0, 0.0, 1.0 / 255.0, 1.0]
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn decode_image(data: &[u8]) -> Option<ColorImage> {
    let img = image::load_from_memory(data).ok()?.to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    Some(ColorImage::from_rgba_unmultiplied([w, h], &img))
}

/// Decode and bake a circular alpha mask (for avatars).
fn decode_circular(data: &[u8]) -> Option<ColorImage> {
    let img = image::load_from_memory(data).ok()?;
    let size = img.width().min(img.height());
    let img = img.resize_to_fill(size, size, image::imageops::FilterType::Lanczos3);
    let mut rgba = img.to_rgba8();
    let c = size as f32 / 2.0;
    let r2 = c * c;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - c;
            let dy = y as f32 - c;
            if dx * dx + dy * dy > r2 {
                rgba.get_pixel_mut(x, y)[3] = 0;
            }
        }
    }
    Some(ColorImage::from_rgba_unmultiplied(
        [size as usize, size as usize],
        &rgba,
    ))
}

/// Draw text with a 1 px black outline on all four diagonal corners.
fn outlined_text(
    p: &egui::Painter,
    text: &str,
    center: Pos2,
    font: FontId,
    fill: Color32,
    outline: Color32,
) {
    for (dx, dy) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        p.text(
            Pos2::new(center.x + dx, center.y + dy),
            Align2::CENTER_CENTER,
            text,
            font.clone(),
            outline,
        );
    }
    p.text(center, Align2::CENTER_CENTER, text, font, fill);
}

// ─────────────────────────────────────────────────────────────────────────────
//  Win32 overlay setup (Windows only)
// ─────────────────────────────────────────────────────────────────────────────

/// Configure the overlay window via Win32:
///   - WS_EX_LAYERED + WS_EX_TRANSPARENT: color-key transparency + click-through
///   - SetLayeredWindowAttributes with LWA_COLORKEY: DWM makes RGB(1,0,1) transparent
///     at the compositor level — works on all GPU vendors including NVIDIA, unlike
///     per-pixel alpha which requires the GPU to correctly output the alpha channel.
///   - Maximize to fill the current monitor, remove title bar
#[cfg(windows)]
unsafe fn win32_setup_overlay() {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::winuser::*;

    let title: Vec<u16> = OsStr::new("MediaChat")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
    if hwnd.is_null() {
        log::warn!("[overlay] HWND not found — Win32 setup skipped");
        return;
    }

    // Remove title bar / borders
    let style = GetWindowLongW(hwnd, GWL_STYLE);
    SetWindowLongW(
        hwnd,
        GWL_STYLE,
        style & !(WS_CAPTION as i32 | WS_THICKFRAME as i32 | WS_MINIMIZEBOX as i32
            | WS_MAXIMIZEBOX as i32 | WS_SYSMENU as i32),
    );

    // Set extended styles: WS_EX_LAYERED (required for color key) + WS_EX_TRANSPARENT (click-through)
    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
    SetWindowLongW(
        hwnd,
        GWL_EXSTYLE,
        ex_style | WS_EX_LAYERED as i32 | WS_EX_TRANSPARENT as i32,
    );

    // Color key: RGB(1, 0, 1) = COLORREF 0x00010001 (format: 0x00BBGGRR).
    // DWM makes every pixel of this exact color transparent — GPU-independent.
    // Must match the `key` color in update() and the value in clear_color().
    let color_key: u32 = 0x0001_0001; // RGB(1, 0, 1)
    SetLayeredWindowAttributes(hwnd, color_key, 0, LWA_COLORKEY);

    SetWindowPos(
        hwnd,
        std::ptr::null_mut(),
        0, 0, 0, 0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
    );

    ShowWindow(hwnd, SW_MAXIMIZE);
    log::info!("[overlay] Win32 color-key overlay setup complete (HWND={hwnd:?}, key=RGB(1,0,1))");
}

/// Parse a CSS hex colour string (#rrggbb or #rgb) → egui Color32.
fn parse_color(s: Option<&str>) -> Option<Color32> {
    let s = s?.trim_start_matches('#');
    match s.len() {
        6 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            Some(Color32::from_rgb(r, g, b))
        }
        3 => {
            let r = u8::from_str_radix(&s[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&s[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&s[2..3], 16).ok()? * 17;
            Some(Color32::from_rgb(r, g, b))
        }
        _ => None,
    }
}
