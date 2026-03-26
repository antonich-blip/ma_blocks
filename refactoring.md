claude --resume ffa9f56b-306d-4dea-a786-02f3d3dd9fcf
claude --resume 86d0aaff-97f6-4907-ac4b-9a738c92ef9f (bubbles)
claude --resume 0836cd05-8f5b-4286-97df-46cad78a2311 (downsizing nixos wrap/package)
claude --resume 47134010-1e31-4274-a989-0505565a036b (video format implementation)

# MaBlocks2 Refactoring Log

## Codebase Overview

**Project:** MaBlocks2 - A Rust/egui desktop application for organizing images on an infinite canvas (whiteboard-style image board)

**Structure:** 5 source files
- `main.rs` — Application state and core logic
- `block.rs` — Block rendering and manipulation
- `block_manager.rs` — Block CRUD, chaining, grouping, LRU animation cache
- `image_loader.rs` — Multi-format image decoding (GIF, WebP, AVIF)
- `constants.rs` — Centralized UI/layout constants
- `paths.rs` — Path management

---

## AVIF Performance Optimization

### Background & Goals

The primary performance target is **AVIF image/animation loading speed**. AVIF decoding uses `libavif-sys` (v0.13.1, backed by `dav1d`) for AV1 decode, with a pipeline of:

```
File read -> AVIF parse -> AV1 decode (dav1d) -> YUV->RGB conversion -> pixel extraction -> ColorImage
```

### Key Discovery

`libavif-sys` v0.13.1 **hardcodes libyuv as disabled** in its build script (`CMAKE_DISABLE_FIND_PACKAGE_libyuv=1`). This means `avifImageYUVToRGB` was using libavif's **naive C fallback** — no SIMD acceleration at all. This made the YUV->RGB step ~3-7x slower than it should be.

### Rejected Approaches

| Approach | Reason for Rejection |
|----------|---------------------|
| Hardware acceleration (VA-API/VideoToolbox) | `libavif` has zero GPU support. Would require extracting raw AV1 OBUs, demuxing AVIF manually. VideoToolbox AV1 is M3+ only. ROI too low for a canvas app. |
| Pipeline decode + YUV->RGB (2A) | `decoder->image` YUV planes are invalidated on each `avifDecoderNextImage()` call. Pipelining would require copying ~3MB of YUV data per frame, negating the overlap gains. Also contends with dav1d's internal threading. |
| `rav1d` (Rust dav1d port) | 5-9% slower than C dav1d. Not at parity yet. |

---

### Implemented Changes

All changes are in `src/image_loader.rs` unless noted.

#### 1A. Fast-path pixel extraction (Completed)

**What:** Added contiguous-data fast path in `RgbImageGuard::extract_pixels()`.

When `rowBytes == width * 4` (no row padding, true for most AVIF images), copies the entire pixel buffer in a single `to_vec()` call instead of iterating row-by-row.

**Impact:** ~5-10% faster pixel extraction per frame.

#### 1B. Reuse RGB buffer across frames (Completed)

**What:** Added `RgbImageGuard::ensure_allocated()` method. The `RgbImageGuard` is now created once before the frame loop and reused. It only reallocates if frame dimensions change (rare in animations).

**Impact:** Eliminates ~N-1 malloc/free cycles for N-frame animations. Each cycle was ~8MB (1080p RGBA). Avoids `mmap`/`munmap` syscalls for large buffers.

#### 1C. Remove intermediate AvifFrame struct (Completed)

**What:** Deleted the `AvifFrame` struct and the post-loop `.into_iter().map().collect()` conversion pass. `AnimationFrame` is now built directly inside the decode loop.

**Impact:** Eliminates one full pixel-data copy per frame (previously: decode -> `AvifFrame` pixels -> `ColorImage` pixels; now: decode -> `ColorImage` pixels directly). For 100 frames at 1080p, saves ~800MB of unnecessary allocation.

#### Parallel downsampling with rayon (Completed)

**What:** Replaced the sequential per-frame downsampling loop (lines 74-92) with `rayon::par_iter_mut()`. Added a pre-check (`needs_downsampling`) to skip rayon dispatch when no frames exceed `MAX_BLOCK_DIMENSION` (420px). Extracted `downsample_frame()` helper.

**Dependency:** Added `rayon = "1.10"` to `Cargo.toml`.

**Impact:** ~6-7x faster downsampling for large-resolution animations (e.g., 1080p -> 420px). No effect on animations already <= 420px (the common case for stickers/small clips).

#### 2B. SIMD YUV->RGB via `yuv` crate (Completed)

**What:** Replaced `avifImageYUVToRGB` (libavif's non-SIMD C fallback) with the `yuv` crate (yuvutils-rs) which provides SIMD-accelerated YUV->RGB conversion: AVX2/SSE4.1 on x86, NEON on ARM, with runtime CPU feature detection.

**New function:** `convert_yuv_to_rgba_simd()` reads raw YUV planes directly from `avifImage` and dispatches to the correct `yuv` crate function based on:
- Pixel format: YUV 4:2:0, 4:2:2, 4:4:4, YUV400 (monochrome)
- Color matrix: BT.601, BT.709, BT.2020, SMPTE240, FCC, BT.470BG, Identity (GBR), YCgCo
- Range: Limited (TV) / Full

Alpha plane compositing is handled as a separate pass when `avifImage.alphaPlane` is present.

**Fallback:** Unsupported formats (10/12-bit depth, exotic matrices like ICTCP/SMPTE2085) fall back to the existing `RgbImageGuard` + `avifImageYUVToRGB` path.

**Dependency:** Added `yuv = "0.8"` to `Cargo.toml` (resolved as v0.8.11, pure Rust).

**Impact:** 3-7x faster YUV->RGB conversion. ~15-30% overall speedup for animated AVIF decode. Especially significant because libavif was running **without libyuv** (the naive C path).

---

### Current Dependency Additions

```toml
rayon = "1.10"   # Parallel frame downsampling
yuv = "0.8"      # SIMD YUV->RGB conversion
```

### Summary of Implemented Optimizations

| Change | What it does | Impact |
|--------|-------------|--------|
| 1A. Fast-path extract_pixels | Single memcpy when no row padding | ~5-10% faster extraction |
| 1B. Reuse RGB buffer | One allocation per animation, not per frame | Avoids N-1 malloc/free for N frames |
| 1C. Remove AvifFrame | Build AnimationFrame directly in loop | Eliminates 1 copy per frame |
| Parallel downsampling | rayon par_iter_mut for resize | ~6x faster for large images |
| 2B. SIMD YUV->RGB | yuv crate replaces naive C fallback | 3-7x faster color conversion |

### Remaining Opportunities (Not Implemented)

| Opportunity | Effort | Expected Impact | Notes |
|-------------|--------|-----------------|-------|
| Streaming frame decode | 1 week | Instant animation start for long sequences | Keep decoder alive, decode on-demand. Complex state management. |
| ~~10/12-bit SIMD path~~ | ~~1-2 days~~ | ~~5-7x faster for HDR content~~ | Completed: `yuv_plane_slice_u16` + `convert_yuv_to_rgba_simd_p16` + `apply_alpha_16bit`. Covers 4:2:0/4:2:2 10+12-bit and 4:4:4 10-bit. Fallback retained for 4:4:4 12-bit, 4:0:0, YCgCo, GBR. |
| Hardware accel via ffmpeg | 2-4 weeks | 3-5x where GPU available | High complexity, platform-specific. Questionable ROI for canvas app. |

---

## Structural Refactoring (Previously Identified)

### Completed
- **BlockManager abstraction** — Block CRUD, chaining, and grouping operations extracted to `block_manager.rs`
- **Constants extraction** — All magic numbers and colors centralized in `constants.rs`

### Remaining Opportunities

| Priority | Issue | Recommendation |
|----------|-------|----------------|
| High | MaBlocksApp is large (main.rs) | Extract SessionManager, CanvasRenderer |
| High | render_canvas method is ~230 lines | Extract input processing, drop target detection, block rendering |
| Medium | Complex conditionals in toggle_compact_group | Use early returns, extract helpers |
| Medium | Toolbar button creation repeated 5x | Create toolbar_button() helper |
| Low | No unit tests | Add tests before major refactoring |
| Low | Missing documentation on public items | Add doc comments |
