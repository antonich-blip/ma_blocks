Codebase Overview
Project: MaBlocks2 - A Rust/egui desktop application for organizing images on an infinite canvas (whiteboard-style image board)
Structure: 4 source files (~2,460 lines total)
- main.rs (1,381 lines) - Application state and core logic
- block.rs (621 lines) - Block rendering and manipulation
- image_loader.rs (428 lines) - Multi-format image decoding
- paths.rs (30 lines) - Path management
---
Redundancy Analysis
High Priority Duplications
| Issue | Description | Locations |
|-------|-------------|-----------|
| Block lookup by ID | blocks.iter().position(\|b\| b.id == id) pattern | 5+ occurrences in main.rs, block.rs |
| Group texture creation | Identical texture creation code | box_group(), data_to_block() |
| World position calculation | (m_pos - canvas_origin) / zoom formula | 3 locations in main.rs |
Medium Priority Duplications
| Issue | Description |
|-------|-------------|
| Y-quantization sorting logic | Identical comparison logic for block positioning appears twice |
| Color restoration from array | Same Color32::from_rgba_unmultiplied pattern duplicated |
| Block property restoration | Position/size assignment duplicated for groups and images |
| Chained blocks filtering | blocks.iter().filter(\|b\| b.chained) appears 3 times |
---
Refactoring Opportunities
High Priority
1. MaBlocksApp is a God class (main.rs:102-1381)
   - Handles UI, persistence, input, animations, chaining, zoom all in one struct
   - Extract: SessionManager, CanvasRenderer, BlockManager, AnimationController
2. render_canvas method is 232 lines (main.rs:795-1027)
   - Extract: process_input_snapshot, detect_drop_target, render_blocks
3. ImageBlock::render is 172 lines (block.rs:363-535)
   - Extract: render_group_background, render_control_buttons, render_counter_badge
4. Magic numbers scattered throughout (block.rs)
   - 16.0, 4.0, 1.2, 6.0, 0.9, etc. used without constants
   - Colors like Color32::from_rgb(100, 100, 150) need semantic names
5. No block lookup abstraction
   - Create block_index(&self, id: Uuid) -> Option<usize> helper
   - Consider HashMap<Uuid, usize> for O(1) lookup
Medium Priority
| Issue | File | Recommendation |
|-------|------|----------------|
| Complex conditionals in toggle_compact_group | main.rs:550-598 | Extract helper methods, use early returns |
| Toolbar button creation repeated 5x | main.rs:733-791 | Create toolbar_button() helper |
| Inconsistent error handling | main.rs | Mix of unwrap(), logging, silent failures |
| data_to_block duplication | main.rs:1247-1310 | Extract deserialize_group_block, deserialize_image_block |
| Tightly coupled ImageBlock to egui | block.rs | Separate domain model from view model |
Low Priority
- Missing documentation on public items
- No unit tests in the project
- session_file field is set but never read
- Some type annotations could be more explicit
---
Recommended Action Plan
1. Add BlockManager abstraction - Consolidate block CRUD and chaining operations
5. Extract session management - Separate persistence from UI logic
6. Add tests - Required before major refactoring

----

## Revised Plan: Critical Analysis & Practical Improvements
Problems with the Original Proposal
Let me be direct about what was wrong with my initial plan:
1. Hardware acceleration was overpromised. libavif has zero hardware acceleration support — confirmed by maintainers. There's no path from AVIF container → GPU decode without extracting raw AV1 OBUs first, which adds complexity that may negate the gains.
2. macOS VideoToolbox AV1 is M3+ only. This means Intel Macs, M1, and M2 get nothing. The Rust crate ecosystem for VideoToolbox AV1 is extremely immature (one canary crate: shiguredo_video_toolbox).
3. VA-API on Linux is viable but requires AVIF demuxing. cros-codecs (by Google/ChromeOS) is production-quality for raw AV1 VA-API decode, but you'd need to extract AV1 OBUs from the AVIF container yourself — libavif doesn't expose them.
4. The "10x speedup" estimates were fantasy. Most of the decode time is already well-optimized — your code uses dav1d (the gold standard) with maxThreads set correctly.
5. The original plan ignored where the real bottleneck is. For your use case (short/long animations in an interactive canvas app), the bottleneck isn't primarily raw AV1 decode speed — it's the __YUV→RGB conversion per frame and the serial frame loop.__
---

### Where Time is Actually Spent (Your Current Pipeline)

   For a 100-frame animated AVIF at 1080p:

| Step | Approx Time | Where |
|------|-------------|-------|
| File read | ~5ms | fs::read() |
| AVIF parse | ~1ms | avifDecoderParse |
| AV1 decode (per frame) | ~5-15ms | avifDecoderNextImage (dav1d, multi-threaded) |
| YUV→RGB (per frame) | ~1-3ms | avifImageYUVToRGB (libavif's internal libyuv) |
| Pixel extraction (per frame) | ~0.5-1ms | extract_pixels row-by-row copy |
| ColorImage creation (per frame) | ~0.5ms | ColorImage::from_rgba_unmultiplied |
| Total for 100 frames | ~700ms-1.9s | Sequential loop |


The decode itself (dav1d) is ~70% of the time and already multi-threaded. The remaining ~30% is YUV conversion + pixel shuffling, done serially per frame.
---
Revised Improvement Plan (Realistic, Ordered by Impact)
Tier 1: Quick Wins (1-3 days, 15-30% faster)
1A. Eliminate redundant pixel copy in extract_pixels
Current code at image_loader.rs:397-414 does a row-by-row copy. If rowBytes == width * 4 (no padding), this is a wasted copy — the data is already contiguous. This is likely true for most AVIF images.
// Current: always copies row-by-row
fn extract_pixels(&self) -> Vec<u8> {
    let mut packed_pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        // row-by-row copy even when contiguous
    }
}
// Improved: fast path for contiguous data
fn extract_pixels(&self) -> Vec<u8> {
    if self.rgb.rowBytes == self.rgb.width * 4 {
        // Zero-copy: just take the whole buffer
        unsafe {
            std::slice::from_raw_parts(
                self.rgb.pixels,
                (self.rgb.width * self.rgb.height * 4) as usize,
            ).to_vec()
        }
    } else {
        // Existing row-by-row path for padded images
        // ...
    }
}
1B. Reuse RgbImageGuard across frames
Currently at image_loader.rs:264-267, a new RgbImageGuard is allocated and freed per frame. For animations where all frames have the same dimensions, allocate once and reuse:
// Current: allocate per frame
while (frame_index as usize) < limit {
    let mut rgb = RgbImageGuard::new();  // allocation
    rgb.allocate(image);                  // malloc
    rgb.convert_from_yuv(image);
    let pixels = rgb.extract_pixels();
    // rgb drops here — free
}
// Improved: allocate once, reuse
let mut rgb = RgbImageGuard::new();
let mut first = true;
while (frame_index as usize) < limit {
    if first {
        rgb.allocate(image);
        first = false;
    }
    rgb.convert_from_yuv(image);
    let pixels = rgb.extract_pixels();
    // rgb stays alive
}
1C. Skip intermediate AvifFrame struct
The code decodes into AvifFrame (lines 292-298), collects all frames into a Vec<AvifFrame>, then converts to Vec<AnimationFrame> (lines 311-321). This is an extra allocation pass. Convert directly to AnimationFrame inside the loop.
---
Tier 2: Medium Wins (3-5 days, 20-40% faster for animations)
2A. Pipeline decode + YUV→RGB conversion
Use a two-thread pipeline: Thread A decodes AV1 frames (dav1d), Thread B converts YUV→RGB. This overlaps the two most expensive operations:
Frame 1: [====DECODE====][===YUV→RGB===]
Frame 2:                  [====DECODE====][===YUV→RGB===]
Frame 3:                                   [====DECODE====][===YUV→RGB===]
// With pipeline:
Frame 1: [====DECODE====][===YUV→RGB===]
Frame 2:                  [====DECODE====]
                          [===YUV→RGB(1)===]
Frame 3:                                   [====DECODE====]
                                           [===YUV→RGB(2)===]
Constraint: avifDecoderNextImage is sequential (inter-frame dependencies), so decode must be serial. But YUV→RGB of frame N can run while decoding frame N+1.
Implementation: Use std::sync::mpsc channel between decode thread and conversion thread. This is simpler than rayon for this producer-consumer pattern.
2B. Replace libavif's YUV→RGB with yuvutils-rs (yuv crate)
The yuv crate benchmarks as 1.5-7x faster than libyuv for YUV→RGB conversion, with full SIMD on both ARM (NEON) and x86 (AVX2/SSE4.1). This would directly accelerate the ~30% of time spent in avifImageYUVToRGB.
Approach: After avifDecoderNextImage, read the raw YUV planes from avifImage directly and feed them to yuvutils-rs instead of calling avifImageYUVToRGB.
// Instead of: avifImageYUVToRGB(image, &mut self.rgb)
// Directly read YUV planes from image and use yuv crate
unsafe {
    let img = &*image;
    let y_plane = slice::from_raw_parts(img.yuvPlanes[0], ...);
    let u_plane = slice::from_raw_parts(img.yuvPlanes[1], ...);
    let v_plane = slice::from_raw_parts(img.yuvPlanes[2], ...);
    // Use yuvutils-rs for SIMD-accelerated conversion
    yuv420_to_rgba(y_plane, u_plane, v_plane, &mut rgba_buf, ...);
}
Trade-off: More code to handle different chroma subsampling formats (4:2:0, 4:2:2, 4:4:4) and color matrices. But significant speedup, especially on 10-bit content where libyuv has no SIMD paths at all.
---
Tier 3: Architecture Improvements (1-2 weeks)
3A. Streaming frame decode for long animations
For animations with 100+ frames, instead of decoding all frames upfront, decode on-demand as the animation plays. This would:
- Reduce initial load time from seconds to near-instant (just first frame)
- Reduce peak memory usage
- Work well with your existing LRU cache
Implementation: Keep the avifDecoder alive and decode frames lazily, buffering a few frames ahead of the playback position.
Trade-off: Significantly more complex state management. The decoder must remain alive for the lifetime of the animation. Must handle seek/restart. But transforms "wait 5 seconds for 500 frames" into "start instantly, decode in background."
3B. Hardware acceleration via ffmpeg-next (Optional, High Effort)
If you want to pursue hardware acceleration despite the complexity:
- Best path: ffmpeg-next v8.0 with VA-API (Linux) / VideoToolbox (macOS M3+)
- Approach: Use ffmpeg to both demux AVIF container AND hardware-decode AV1
- Fallback: Current libavif-sys + dav1d software path
- Complexity: High — requires unsafe FFI for hwaccel context setup, platform-specific build configuration, runtime detection
Honest assessment: This is 2-4 weeks of work for a 3-5x speedup that only applies on machines with hardware AV1 support. For your whiteboard app (not a video player), the ROI is questionable.
---
Updated Dependency Changes
```sh
# Add to Cargo.toml
[dependencies]
yuv = "0.8"  # SIMD YUV→RGB conversion (Tier 2B)
# rayon = "1.10"  # Only if going with Tier 2A parallel pipeline
No other dependency changes needed for Tier 1 and Tier 2.

```
---

Summary: Recommended Implementation Order

| Priority | Change | Effort | Speedup | Risk |
|----------|--------|--------|---------|------|
| 1 | Eliminate redundant pixel copy (1A) | 30 min | 5-10% | None |
| 2 | Reuse RGB buffer across frames (1B) | 1 hour | 5-10% | Low |
| 3 | Remove intermediate AvifFrame (1C) | 1 hour | 2-5% | None |
| 4 | Pipeline decode + conversion (2A) | 1-2 days | 15-25% | Medium |
| 5 | SIMD YUV→RGB via yuv crate (2B) | 2-3 days | 10-30% | Medium |
| 6 | Streaming frame decode (3A) | 1 week | UX: instant start | High |
| 7 | Hardware accel via ffmpeg (3B) | 2-4 weeks | 3-5x (where available) | High |

My recommendation: Implement Tiers 1-2 (items 1-5). That gives you roughly 30-50% faster animation loading with moderate effort and zero platform-specific code. Tier 3 items are optional and depend on whether the remaining performance is actually a user-facing problem.

