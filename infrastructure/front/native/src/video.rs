/// Video decoder backed by system FFmpeg 4.x.
///
/// System deps (Debian/Ubuntu):
///   sudo apt install libavcodec-dev libavformat-dev libavutil-dev \
///                    libswscale-dev libswresample-dev
///
/// Audio is handled separately by spawning `ffplay -nodisp -autoexit` on the
/// downloaded temp file, so no audio dev libraries are needed here.
use std::sync::mpsc::SyncSender;

use anyhow::Result;
use ffmpeg_next as ffmpeg;
use ffmpeg::format::Pixel;
use ffmpeg::media::Type;
use ffmpeg::software::scaling::{context::Context as ScaleCtx, flag::Flags as ScaleFlags};

use crate::types::{AppEvent, VideoFrame};

/// Number of decoded frames buffered in the channel before back-pressure kicks in.
/// 15 frames ≈ 0.5 s at 30 fps; at 720p RGBA ≈ 53 MB.
const FRAME_BUF: usize = 15;

/// Spawn the video decode pipeline in a background thread.
///
/// Flow:
///   1. Download URL → named temp file (blocking in the background thread).
///   2. Send `AppEvent::VideoReady { frame_rx, audio_path }` to the app.
///   3. Decode video frames into `frame_rx` (bounded, FRAME_BUF capacity).
///   4. Send `AppEvent::VideoEnded` when done.
///
/// The temp file is kept alive until the background thread returns (video is
/// fully decoded).  On Linux, even after `remove_file`, any process that
/// already has the file open (e.g. ffplay) continues to read it fine.
pub fn spawn_video_decoder(url: String, event_tx: std::sync::mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || {
        if let Err(e) = pipeline(url, event_tx.clone()) {
            log::error!("Video pipeline error: {e}");
            let _ = event_tx.send(AppEvent::VideoEnded);
        }
    });
}

fn pipeline(url: String, event_tx: std::sync::mpsc::Sender<AppEvent>) -> Result<()> {
    // ── download ─────────────────────────────────────────────────────────────
    log::info!("Downloading video: {url}");
    let bytes = reqwest::blocking::get(&url)?.bytes()?;

    let tmp = tempfile::NamedTempFile::new()?;
    let (mut tmp_file, tmp_path) = tmp.keep()?;
    std::io::Write::write_all(&mut tmp_file, &bytes)?;
    drop(tmp_file); // close write handle; ffmpeg opens its own

    let path = tmp_path.to_string_lossy().to_string();

    // ── probe for audio ───────────────────────────────────────────────────────
    let has_audio = {
        let pb = std::path::PathBuf::from(&path);
        let ictx = ffmpeg::format::input(&pb)?;
        ictx.streams().best(Type::Audio).is_some()
    };
    let audio_path = if has_audio { Some(path.clone()) } else { None };

    // ── hand off frame channel to the app ─────────────────────────────────────
    let (frame_tx, frame_rx) = std::sync::mpsc::sync_channel::<VideoFrame>(FRAME_BUF);
    let _ = event_tx.send(AppEvent::VideoReady { frame_rx, audio_path });

    // ── decode video frames ───────────────────────────────────────────────────
    decode_video(&path, frame_tx)?;
    let _ = event_tx.send(AppEvent::VideoEnded);

    // Clean up temp file.  ffplay already has its file descriptor open and
    // will continue reading on Linux even after the directory entry is gone.
    let _ = std::fs::remove_file(&tmp_path);
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────

fn decode_video(path: &str, tx: SyncSender<VideoFrame>) -> Result<()> {
    let pb = std::path::PathBuf::from(path);
    let mut ictx = ffmpeg::format::input(&pb)?;

    let video_idx = ictx
        .streams()
        .best(Type::Video)
        .ok_or_else(|| anyhow::anyhow!("no video stream"))?
        .index();

    let time_base = {
        let tb = ictx.stream(video_idx).unwrap().time_base();
        tb.numerator() as f64 / tb.denominator() as f64
    };

    let mut decoder = {
        let stream = ictx.stream(video_idx).unwrap();
        let mut ctx = ffmpeg::codec::context::Context::new();
        ctx.set_parameters(stream.parameters())?;
        ctx.decoder().video()?
    };

    let mut scaler = ScaleCtx::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        Pixel::RGBA,
        decoder.width(),
        decoder.height(),
        ScaleFlags::BILINEAR,
    )?;

    let mut raw = ffmpeg::util::frame::video::Video::empty();
    let mut rgba = ffmpeg::util::frame::video::Video::empty();

    for (stream, packet) in ictx.packets() {
        if stream.index() != video_idx {
            continue;
        }
        decoder.send_packet(&packet)?;

        while decoder.receive_frame(&mut raw).is_ok() {
            scaler.run(&raw, &mut rgba)?;
            let pts_secs = raw.pts().unwrap_or(0) as f64 * time_base;

            let frame = VideoFrame {
                width: rgba.width(),
                height: rgba.height(),
                data: rgba.data(0).to_vec(),
                pts_secs,
            };

            // send() blocks here when FRAME_BUF is full — natural back-pressure
            if tx.send(frame).is_err() {
                return Ok(()); // app navigated away, receiver dropped
            }
        }
    }

    // Flush remaining frames held inside the decoder
    decoder.send_eof()?;
    while decoder.receive_frame(&mut raw).is_ok() {
        scaler.run(&raw, &mut rgba)?;
        let pts_secs = raw.pts().unwrap_or(0) as f64 * time_base;
        let frame = VideoFrame {
            width: rgba.width(),
            height: rgba.height(),
            data: rgba.data(0).to_vec(),
            pts_secs,
        };
        if tx.send(frame).is_err() {
            break;
        }
    }

    Ok(())
}
