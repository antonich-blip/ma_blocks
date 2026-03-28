# WebM/VP9 Streaming — Implementation Plan

## Goal

Replace pre-decoded `Vec<AnimationFrame>` with per-block streaming decoders for WebM/VP9 files.
- Static blocks: show first frame only (~700KB RAM each)
- Playing blocks: stream decode on-the-fly (~2MB RAM each)
- 40 playing blocks: ~80MB total vs ~840MB current

GIF/WebP/AVIF/PNG/JPG paths are untouched.

---

## Dependencies

**flake.nix first** — add to `buildInputs`/`runtimeLibs` in both `devShells.default` and `packages.default`:
```nix
pkgs.ffmpeg
pkgs.ffmpeg.dev
```
Validate: `pkg-config --libs libavcodec`

**Cargo.toml:**
```toml
ffmpeg-next = { version = "8.1", default-features = false, features = ["codec", "format", "software-scaling"] }
```

---

## Critical Constraint: `!Send`

`ffmpeg::format::context::Input` and `codec::decoder::Video` wrap raw C pointers — they are `!Send`.

**Rule:** create and use them exclusively on the background thread. Only `Arc<Mutex<...>>` and `mpsc` channels cross thread boundaries.

---

## New File: `src/video_stream.rs`

All ffmpeg logic isolated here. No other module touches ffmpeg-next directly.

### Public types (Send — cross thread boundary)

```rust
pub struct VideoBlockHandle {
    cmd_tx: mpsc::Sender<StreamCmd>,
    latest_frame: Arc<Mutex<Option<DecodedVideoFrame>>>,
    frame_duration: Duration,
}

pub enum StreamCmd { Play, Pause, Seek(f64), Drop }

pub struct DecodedVideoFrame {
    image: ColorImage,
    pts: Duration,
}
```

### Private types (!Send — background thread only)

```rust
struct VideoStreamDecoder {
    format_ctx: ffmpeg::format::context::Input,
    decoder: ffmpeg::codec::decoder::Video,
    scaler: ffmpeg::software::scaling::Context,  // YUV → RGBA (AV_PIX_FMT_RGBA)
    stream_index: usize,
}
```

### Background thread loop

1. Read packets from `format_ctx`
2. Decode via `decoder`, scale to RGBA via `scaler`
3. Write result into `Arc<Mutex<Option<DecodedVideoFrame>>>`
4. On EOF: `format_ctx.seek(0, ..)` + `decoder.flush()` → seamless loop
5. On `cmd_rx` disconnect (handle dropped): return → all ffmpeg state drops on owning thread

### Also in this file

`pub fn load_video_first_frame(path) -> Result<LoadedImage, String>`
Opens format context, decodes exactly one frame, drops everything. Returns single-frame `LoadedImage` with `has_animation: true`.

`pub fn spawn_video_decoder(path: PathBuf) -> VideoBlockHandle`
Spawns background thread, returns handle.

---

## File Changes (in order)

### 1. `flake.nix`
Add ffmpeg to buildInputs/runtimeLibs.

### 2. `Cargo.toml`
Add ffmpeg-next. Run `cargo check`.

### 3. `src/video_stream.rs` (new)
All ffmpeg logic. No changes needed elsewhere until this compiles.

### 4. `src/image_loader.rs`
```rust
pub fn is_video_format(path: &Path) -> bool {
    matches!(path.extension().and_then(|e| e.to_str()),
        Some("webm" | "mp4" | "mkv" | "mov"))
}
```
Add early-return branch at top of `load_image_frames_scaled`:
```rust
if is_video_format(path) {
    return video_stream::load_video_first_frame(path);
}
// existing match format { ... } unchanged
```

### 5. `src/block.rs`
Add to `AnimationState`:
```rust
pub video: Option<crate::video_stream::VideoBlockHandle>,
```
`update_animation`: check `self.video` first (pull latest frame from mutex, update texture), fall through to existing Vec path only if `video.is_none()`.

`stop_animation`: if `video.is_some()`, send `StreamCmd::Pause`.

### 6. `src/block_manager.rs`
`purge_animation_frames`: for video blocks, drop `block.anim.video` (sends disconnect to background thread) instead of truncating `frames`. The `Vec<AnimationFrame>` for video blocks always has exactly 1 frame (the first frame) — never truncate it.

### 7. `src/main.rs`
- `main()`: add `ffmpeg_next::init()` before `eframe::run_native`
- `handle_block_click`: inside the `!block.is_full_sequence` arm, add video branch:
  ```rust
  if image_loader::is_video_format(&block.path) {
      block.anim.video = Some(video_stream::spawn_video_decoder(block.path.clone()));
      block.anim.animation_enabled = true;
      self.block_manager.mark_animation_used(id);
  } else {
      // existing trigger_image_load path
  }
  ```
- File dialog filter: add `"webm"` to extensions list

---

## Memory Model

| State | RAM per block |
|---|---|
| Static (first frame) | ~700KB CPU + ~700KB GPU texture |
| Playing | ~2MB (decoder + scaler buffers + 1 frame) |
| 40 blocks playing | ~80MB |
| 20 blocks playing (current LRU cap, pre-decoded WebP) | ~840MB |

LRU eviction (`MAX_CACHED_ANIMATIONS = 20` in `constants.rs`) applies to video blocks through existing `mark_animation_used` / `purge_animation_frames` — no constant changes needed.

---

## Key Gotchas

- **`ffmpeg_next::init()`** must be called once in `main()` before any other ffmpeg call
- **After seek-to-zero**: always call `decoder.flush()` (`avcodec_flush_buffers`) or stale frames appear
- **Reuse frame allocations**: use `frame::Video::empty()` outside the loop and reuse each iteration — avoids per-frame `av_frame_alloc`
- **SwsContext output format**: `AV_PIX_FMT_RGBA` → `rgba_frame.data(0)` is exactly `width * height * 4` bytes, directly usable by `egui::ColorImage::from_rgba_unmultiplied`
- **Shutdown ordering**: drop `VideoBlockHandle` (disconnects `cmd_tx`) → background thread's `cmd_rx.try_recv()` returns `Disconnected` → thread returns → ffmpeg structs drop on owning thread. This is the only safe ordering.
- **VP9 seek accuracy**: VP9 in WebM always has a keyframe at pts 0, so `seek(0, ..)` with `AVSEEK_FLAG_BACKWARD` reliably lands on frame 0

---

## Out of Scope

- Hardware decode (VA-API/NVDEC) — software VP9 at 420px is cheap
- Audio tracks — ignored (filter by video stream index only)
- MP4/H.264 — architecture supports it, just add to `is_video_format` and file dialog filter
- Seeking by user (scrubbing) — `StreamCmd::Seek(f64)` enum variant is a forward-compatible hook
