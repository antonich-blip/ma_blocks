//! Per-block streaming video decoder for WebM/VP9 (and other ffmpeg-supported formats).
//!
//! Each playing block gets a background thread that owns the ffmpeg context
//! (which is !Send) and writes decoded RGBA frames into an Arc<Mutex<...>>.
//! The render loop reads the latest frame each tick without blocking.
//!
//! Memory model:
//! - Static block (not playing): only the first frame ColorImage in RAM (~700KB)
//! - Playing block: ~2MB (decoder + scaler buffers + one ColorImage)

use egui::ColorImage;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// ─────────────────────────────────────────────────────────────────────────────
// Public types (all Send — safe to store on ImageBlock)
// ─────────────────────────────────────────────────────────────────────────────

/// Commands sent from the main thread to the background decoder thread.
// Drop/shutdown is implicit: when VideoBlockHandle is dropped, the Sender
// disconnects and the decoder thread exits on the next TryRecvError::Disconnected.
pub enum StreamCmd {
    Play,
    Pause,
}

/// A single decoded RGBA frame plus a monotonic sequence number.
/// The seq counter lets `update_animation` detect whether a new frame arrived.
pub struct DecodedVideoFrame {
    pub image: ColorImage,
    pub seq: u64,
}

/// Stored on `AnimationState`. All fields are Send.
pub struct VideoBlockHandle {
    pub cmd_tx: std::sync::mpsc::Sender<StreamCmd>,
    pub latest_frame: Arc<Mutex<Option<DecodedVideoFrame>>>,
    /// Nominal frame duration, used for repaint scheduling.
    pub frame_duration: Duration,
    /// First frame image, restored to texture when playback stops.
    pub first_frame: ColorImage,
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Returns true if `path` has a video container extension supported by ffmpeg.
pub fn is_video_format(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("webm" | "mp4" | "mkv" | "mov")
    )
}

/// Spawns a background decoder thread and returns a handle to it.
/// The decoder starts in the Paused state; call `cmd_tx.send(StreamCmd::Play)` to begin.
pub fn spawn_video_decoder(path: PathBuf, first_frame: ColorImage) -> VideoBlockHandle {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<StreamCmd>();
    let latest_frame: Arc<Mutex<Option<DecodedVideoFrame>>> = Arc::new(Mutex::new(None));
    let latest_frame_bg = Arc::clone(&latest_frame);

    let frame_duration = probe_frame_duration(&path).unwrap_or(Duration::from_millis(42));
    let first_frame_bg = first_frame.clone();

    std::thread::spawn(move || {
        decoder_thread(path, cmd_rx, latest_frame_bg, first_frame_bg);
    });

    VideoBlockHandle {
        cmd_tx,
        latest_frame,
        frame_duration,
        first_frame,
    }
}

/// Decodes only the first frame of a video file for static display.
/// Returns a single-frame `LoadedImage` with `has_animation = true`.
pub fn load_video_first_frame(path: &Path) -> Result<crate::image_loader::LoadedImage, String> {
    use ffmpeg_next as ff;

    let mut input = ff::format::input(path)
        .map_err(|e| format!("Failed to open {:?}: {e}", path))?;

    let (stream_index, frame_duration, codec_params) = {
        let stream = input
            .streams()
            .best(ff::media::Type::Video)
            .ok_or_else(|| format!("No video stream in {:?}", path))?;
        (stream.index(), frame_duration_from_stream(&stream), stream.parameters())
    };

    let mut decoder = ff::codec::context::Context::from_parameters(codec_params)
        .map_err(|e| format!("Codec context: {e}"))?
        .decoder()
        .video()
        .map_err(|e| format!("Video decoder: {e}"))?;

    let (width, height) = (decoder.width(), decoder.height());
    let src_fmt = decoder.format();

    let mut scaler = ff::software::scaling::context::Context::get(
        src_fmt,
        width,
        height,
        ff::format::pixel::Pixel::RGBA,
        width,
        height,
        ff::software::scaling::flag::Flags::BILINEAR,
    )
    .map_err(|e| format!("Scaler: {e}"))?;

    let mut raw = ff::frame::Video::empty();
    let mut rgba = ff::frame::Video::empty();

    for (stream, packet) in input.packets() {
        if stream.index() != stream_index {
            continue;
        }
        if decoder.send_packet(&packet).is_err() {
            continue;
        }
        if decoder.receive_frame(&mut raw).is_ok() {
            scaler
                .run(&raw, &mut rgba)
                .map_err(|e| format!("Scale first frame: {e}"))?;

            let ci = rgba_frame_to_color_image(&rgba, width, height);
            return Ok(crate::image_loader::LoadedImage::from_frames(
                vec![crate::image_loader::AnimationFrame {
                    image: ci,
                    duration: frame_duration,
                }],
                true,
            ));
        }
    }

    Err(format!("No frames decoded from {:?}", path))
}

// ─────────────────────────────────────────────────────────────────────────────
// Background decoder thread (!Send types live here exclusively)
// ─────────────────────────────────────────────────────────────────────────────

fn decoder_thread(
    path: PathBuf,
    cmd_rx: std::sync::mpsc::Receiver<StreamCmd>,
    latest_frame: Arc<Mutex<Option<DecodedVideoFrame>>>,
    _first_frame: ColorImage,
) {
    use std::sync::mpsc::TryRecvError;

    let mut playing = false;
    let mut seq: u64 = 0;

    'outer: loop {
        // When paused, block until a command arrives rather than spinning.
        if !playing {
            match cmd_rx.recv() {
                Ok(StreamCmd::Play) => playing = true,
                Ok(StreamCmd::Pause) => {}
                Err(_) => break 'outer, // Sender dropped → block deleted/evicted
            }
        }

        // (Re-)open the file on every loop iteration (also handles loop restart after EOF).
        let mut input = match ffmpeg_next::format::input(&path) {
            Ok(i) => i,
            Err(e) => {
                log::error!("video_stream: failed to open {:?}: {e}", path);
                break 'outer;
            }
        };

        let (stream_index, codec_params) = match input.streams().best(ffmpeg_next::media::Type::Video) {
            Some(s) => (s.index(), s.parameters()),
            None => {
                log::error!("video_stream: no video stream in {:?}", path);
                break 'outer;
            }
        };

        let mut decoder =
            match ffmpeg_next::codec::context::Context::from_parameters(codec_params)
                .and_then(|ctx| ctx.decoder().video())
            {
                Ok(d) => d,
                Err(e) => {
                    log::error!("video_stream: decoder init failed: {e}");
                    break 'outer;
                }
            };

        let (width, height) = (decoder.width(), decoder.height());
        let src_fmt = decoder.format();

        let mut scaler = match ffmpeg_next::software::scaling::context::Context::get(
            src_fmt,
            width,
            height,
            ffmpeg_next::format::pixel::Pixel::RGBA,
            width,
            height,
            ffmpeg_next::software::scaling::flag::Flags::BILINEAR,
        ) {
            Ok(s) => s,
            Err(e) => {
                log::error!("video_stream: scaler init failed: {e}");
                break 'outer;
            }
        };

        // Reuse allocations across frames.
        let mut raw = ffmpeg_next::frame::Video::empty();
        let mut rgba = ffmpeg_next::frame::Video::empty();

        for (stream, packet) in input.packets() {
            // Check for commands between packets (non-blocking).
            match cmd_rx.try_recv() {
                Ok(StreamCmd::Pause) => {
                    playing = false;
                    continue 'outer; // return to the blocking recv at top
                }
                Err(TryRecvError::Disconnected) => break 'outer,
                Ok(StreamCmd::Play) | Err(TryRecvError::Empty) => {}
            }

            if stream.index() != stream_index {
                continue;
            }
            if decoder.send_packet(&packet).is_err() {
                continue;
            }
            while decoder.receive_frame(&mut raw).is_ok() {
                if scaler.run(&raw, &mut rgba).is_err() {
                    continue;
                }
                seq += 1;
                let ci = rgba_frame_to_color_image(&rgba, width, height);
                if let Ok(mut guard) = latest_frame.lock() {
                    *guard = Some(DecodedVideoFrame { image: ci, seq });
                }
            }
        }
        // EOF reached — loop back to reopen the file (seamless loop).
        // If paused, the blocking recv at the top of 'outer will wait.
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Probes a video file to determine the nominal frame duration.
/// Returns None on any error; callers should substitute a default.
fn probe_frame_duration(path: &Path) -> Option<Duration> {
    let input = ffmpeg_next::format::input(path).ok()?;
    let stream = input.streams().best(ffmpeg_next::media::Type::Video)?;
    Some(frame_duration_from_stream(&stream))
}

fn frame_duration_from_stream(stream: &ffmpeg_next::format::stream::Stream) -> Duration {
    let r = stream.avg_frame_rate();
    if r.numerator() > 0 && r.denominator() > 0 {
        Duration::from_secs_f64(r.denominator() as f64 / r.numerator() as f64)
    } else {
        Duration::from_millis(42) // ~24 fps fallback
    }
}

fn rgba_frame_to_color_image(
    rgba: &ffmpeg_next::frame::Video,
    width: u32,
    height: u32,
) -> ColorImage {
    let data = rgba.data(0);
    ColorImage::from_rgba_unmultiplied([width as usize, height as usize], data)
}
