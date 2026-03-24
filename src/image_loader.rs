use egui::ColorImage;
use image::codecs::gif::GifDecoder;
use image::codecs::webp::WebPDecoder;
use image::{AnimationDecoder, DynamicImage, Frame, ImageFormat};
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::time::Duration;

/// Maximum number of frames to load for an animation to prevent excessive memory usage.
pub const MAX_ANIMATION_FRAMES: usize = 1024;

/// A single frame of an animated image, including its pixel data and display duration.
#[derive(Clone)]
pub struct AnimationFrame {
    pub image: ColorImage,
    pub duration: Duration,
}

/// Holds all frames and metadata for a loaded image, supporting both static and animated formats.
pub struct LoadedImage {
    pub frames: Vec<AnimationFrame>,
    pub original_size: egui::Vec2,
    pub has_animation: bool,
}

/// Result of an image load operation, containing the path, loaded data, and a flag indicating if it's a full sequence.
pub type ImageLoadResult = (std::path::PathBuf, LoadedImage, bool);
/// Response type for image loading, wrapping the result in a Result with a string error.
pub type ImageLoadResponse = Result<ImageLoadResult, String>;

impl LoadedImage {
    /// Creates a new LoadedImage from a sequence of frames and animation metadata.
    pub fn from_frames(frames: Vec<AnimationFrame>, has_animation: bool) -> Self {
        let original_size = frames
            .first()
            .map(|frame| {
                let [w, h] = frame.image.size;
                egui::vec2(w as f32, h as f32)
            })
            .unwrap_or(egui::vec2(1.0, 1.0));
        Self {
            frames,
            original_size,
            has_animation,
        }
    }
}

/// Loads an image from the specified path, optionally scaling it and loading only the first frame.
/// Supports GIF, WebP, and AVIF formats, falling back to static decoding for other formats.
pub fn load_image_frames_scaled(
    path: &Path,
    max_dimension: Option<u32>,
    first_frame_only: bool,
) -> Result<LoadedImage, String> {
    let bytes =
        fs::read(path).map_err(|err| format!("Failed to read {}: {err}", path.display()))?;

    let format = image::guess_format(&bytes)
        .or_else(|_| ImageFormat::from_path(path))
        .map_err(|err| format!("Failed to determine format for {}: {err}", path.display()))?;

    let mut loaded = match format {
        ImageFormat::Gif => decode_gif(&bytes, first_frame_only),
        ImageFormat::WebP => decode_webp(&bytes, first_frame_only),
        ImageFormat::Avif => decode_avif(&bytes, first_frame_only).or_else(|err| {
            log::warn!("Falling back to static AVIF decode: {err}");
            decode_static(&bytes, ImageFormat::Avif)
        }),
        _ => decode_static(&bytes, format),
    }?;

    if let Some(max_dim) = max_dimension {
        // Check if any frame needs downsampling before paying rayon dispatch cost.
        let needs_downsampling = loaded.frames.iter().any(|frame| {
            let [w, h] = frame.image.size;
            w > max_dim as usize || h > max_dim as usize
        });

        if needs_downsampling {
            use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};

            loaded.frames.par_iter_mut().for_each(|frame| {
                downsample_frame(frame, max_dim);
            });
        }
    }

    Ok(loaded)
}

/// Decodes a static image using the specified format.
fn decode_static(bytes: &[u8], format: ImageFormat) -> Result<LoadedImage, String> {
    let image = image::load_from_memory_with_format(bytes, format)
        .map_err(|err| format!("Failed to decode image: {err}"))?;
    let color_image = color_image_from_dynamic(image);
    Ok(LoadedImage::from_frames(
        vec![AnimationFrame {
            image: color_image,
            duration: Duration::from_millis(1000),
        }],
        false,
    ))
}

/// Decodes a GIF image, optionally loading only the first frame.
fn decode_gif(bytes: &[u8], first_frame_only: bool) -> Result<LoadedImage, String> {
    let cursor = Cursor::new(bytes);
    let decoder = GifDecoder::new(cursor).map_err(|err| format!("GIF decode error: {err}"))?;
    let limit = if first_frame_only {
        1
    } else {
        MAX_ANIMATION_FRAMES
    };
    let frames: Vec<Frame> = decoder
        .into_frames()
        .take(limit)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("GIF frame error: {err}"))?;
    frames_to_loaded_image(frames, true)
}

/// Decodes a WebP image, optionally loading only the first frame if it's animated.
fn decode_webp(bytes: &[u8], first_frame_only: bool) -> Result<LoadedImage, String> {
    let decoder =
        WebPDecoder::new(Cursor::new(bytes)).map_err(|err| format!("WebP decode error: {err}"))?;
    let has_animation = decoder.has_animation();
    if has_animation {
        let limit = if first_frame_only {
            1
        } else {
            MAX_ANIMATION_FRAMES
        };
        let frames: Vec<Frame> = decoder
            .into_frames()
            .take(limit)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("WebP frame error: {err}"))?;
        frames_to_loaded_image(frames, true)
    } else {
        decode_static(bytes, ImageFormat::WebP)
    }
}

/// Decodes an AVIF image using the specialized AVIF support module.
fn decode_avif(bytes: &[u8], first_frame_only: bool) -> Result<LoadedImage, String> {
    avif_support::decode(bytes, first_frame_only)
}

/// Converts a sequence of image frames into a LoadedImage.
fn frames_to_loaded_image(frames: Vec<Frame>, has_animation: bool) -> Result<LoadedImage, String> {
    if frames.is_empty() {
        return Err("Image did not contain frames".to_string());
    }

    let mut converted = Vec::with_capacity(frames.len());
    for frame in frames {
        let delay = duration_from_delay(frame.delay());
        let buffer = frame.into_buffer();
        let size = [buffer.width() as usize, buffer.height() as usize];
        let pixels = buffer.into_raw();
        let image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
        converted.push(AnimationFrame {
            image,
            duration: sanitize_duration(delay),
        });
    }

    Ok(LoadedImage::from_frames(converted, has_animation))
}

/// Helper to convert a DynamicImage into an egui ColorImage.
fn color_image_from_dynamic(image: DynamicImage) -> egui::ColorImage {
    let rgba = image.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    egui::ColorImage::from_rgba_unmultiplied(size, &rgba.into_raw())
}

/// Downsamples a single frame if it exceeds the given maximum dimension.
fn downsample_frame(frame: &mut AnimationFrame, max_dim: u32) {
    let [w, h] = frame.image.size;
    if w > max_dim as usize || h > max_dim as usize {
        let scale = (max_dim as f32) / (w.max(h) as f32);
        let new_w = (w as f32 * scale) as u32;
        let new_h = (h as f32 * scale) as u32;

        let rgba = frame.image.as_raw().to_vec();
        if let Some(img) =
            image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(w as u32, h as u32, rgba)
        {
            let dynamic = DynamicImage::ImageRgba8(img);
            let resized = dynamic.thumbnail(new_w, new_h);
            frame.image = color_image_from_dynamic(resized);
        }
    }
}

/// Calculates a Duration from an image delay, with a minimum value of 1ms.
fn duration_from_delay(delay: image::Delay) -> Duration {
    let (numer, denom) = delay.numer_denom_ms();
    let denom = denom.max(1);
    let millis = (numer as f32) / (denom as f32);
    Duration::from_secs_f32((millis / 1000.0).max(0.001))
}

/// Ensures the duration is non-zero, defaulting to 16ms (roughly 60fps) if zero.
fn sanitize_duration(duration: Duration) -> Duration {
    if duration.is_zero() {
        Duration::from_millis(16)
    } else {
        duration
    }
}

mod avif_support {
    use super::{sanitize_duration, AnimationFrame, LoadedImage, MAX_ANIMATION_FRAMES};
    use egui::ColorImage;
    use std::time::Duration;

    pub fn decode(bytes: &[u8], first_frame_only: bool) -> Result<LoadedImage, String> {
        let decoder =
            DecoderGuard::new().ok_or_else(|| "Failed to create AVIF decoder".to_string())?;

        // SAFETY: decoder.decoder is a valid pointer from DecoderGuard::new(),
        // and bytes.as_ptr() is valid for bytes.len() as it comes from a slice.
        unsafe {
            let result =
                libavif_sys::avifDecoderSetIOMemory(decoder.decoder, bytes.as_ptr(), bytes.len());
            if result != libavif_sys::AVIF_RESULT_OK {
                return Err(format!("avifDecoderSetIOMemory failed: {}", result as i32));
            }

            let result = libavif_sys::avifDecoderParse(decoder.decoder);
            if result != libavif_sys::AVIF_RESULT_OK {
                return Err(format!("avifDecoderParse failed: {}", result as i32));
            }
        }

        // SAFETY: decoder.decoder is a valid pointer to an initialized avifDecoder.
        let image_count = unsafe { (*decoder.decoder).imageCount as usize };
        // SAFETY: decoder.decoder is a valid pointer to an initialized avifDecoder.
        let total_duration = unsafe { (*decoder.decoder).duration };
        let fallback_duration = if image_count > 1 && total_duration > 0.0 {
            total_duration / image_count as f64
        } else if image_count > 1 {
            0.1
        } else {
            0.0
        };

        let mut frames = Vec::new();
        let mut frame_index: u32 = 0;
        // Fallback RGB buffer kept for formats the SIMD path doesn't support.
        let mut rgb_fallback = RgbImageGuard::new();

        let limit = if first_frame_only {
            1
        } else {
            MAX_ANIMATION_FRAMES
        };

        while (frame_index as usize) < limit {
            // SAFETY: decoder.decoder is a valid pointer to an initialized avifDecoder.
            let result = unsafe { libavif_sys::avifDecoderNextImage(decoder.decoder) };
            if result == libavif_sys::AVIF_RESULT_OK {
                // SAFETY: decoder.decoder is a valid pointer to an initialized avifDecoder.
                let image = unsafe { (*decoder.decoder).image };
                if image.is_null() {
                    frame_index += 1;
                    continue;
                }

                // SAFETY: image is checked for null before dereferencing and is owned by the decoder.
                let (width, height) = unsafe { ((*image).width, (*image).height) };
                if width == 0 || height == 0 {
                    frame_index += 1;
                    continue;
                }

                // Try SIMD-accelerated YUV→RGBA first, fall back to libavif's
                // (non-libyuv) C implementation for unsupported formats.
                let pixels = match convert_yuv_to_rgba_simd(image) {
                    Ok(rgba) => rgba,
                    Err(_) => {
                        rgb_fallback.ensure_allocated(image);
                        rgb_fallback.convert_from_yuv(image);
                        rgb_fallback.extract_pixels()
                    }
                };

                // SAFETY: decoder.decoder is valid, and frame_index is within bounds (0..image_count).
                // timing is zero-initialized and passed as a mutable reference.
                let duration_secs = unsafe {
                    let mut timing: libavif_sys::avifImageTiming = std::mem::zeroed();
                    let timing_result = libavif_sys::avifDecoderNthImageTiming(
                        decoder.decoder,
                        frame_index,
                        &mut timing,
                    );
                    if timing_result == libavif_sys::AVIF_RESULT_OK && timing.duration > 0.0 {
                        timing.duration
                    } else {
                        let img_timing = (*decoder.decoder).imageTiming.duration;
                        if img_timing > 0.0 {
                            img_timing
                        } else if image_count > 1 {
                            fallback_duration.max(0.0)
                        } else {
                            0.0
                        }
                    }
                };

                // Build AnimationFrame directly — no intermediate struct needed.
                let size = [width as usize, height as usize];
                let color_image = ColorImage::from_rgba_unmultiplied(size, &pixels);
                frames.push(AnimationFrame {
                    image: color_image,
                    duration: sanitize_duration(Duration::from_secs_f64(duration_secs.max(0.0))),
                });

                frame_index += 1;
            } else if result == libavif_sys::AVIF_RESULT_NO_IMAGES_REMAINING {
                break;
            } else {
                return Err(format!("avifDecoderNextImage failed: {}", result as i32));
            }
        }

        if frames.is_empty() {
            return Err("AVIF decode produced no frames".to_string());
        }

        Ok(LoadedImage::from_frames(frames, image_count > 1))
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // SIMD YUV→RGBA conversion via the `yuv` crate
    // ─────────────────────────────────────────────────────────────────────────────

    /// Maps a libavif `avifRange` constant to a `yuv` crate `YuvRange`.
    fn map_range(range: libavif_sys::avifRange) -> yuv::YuvRange {
        if range == libavif_sys::AVIF_RANGE_FULL {
            yuv::YuvRange::Full
        } else {
            yuv::YuvRange::Limited
        }
    }

    /// Maps a libavif `avifMatrixCoefficients` constant to a `yuv` crate `YuvStandardMatrix`.
    /// Returns `None` for Identity (0) and YCgCo (8) which need special dispatch,
    /// and for truly unsupported matrices.
    fn map_matrix(mc: u16) -> Option<yuv::YuvStandardMatrix> {
        // Constants from libavif-sys 0.13:
        // 0 = Identity, 1 = BT709, 2 = Unspecified, 4 = FCC, 5 = BT470BG,
        // 6 = BT601, 7 = SMPTE240, 8 = YCgCo, 9 = BT2020_NCL
        match mc as u32 {
            1 => Some(yuv::YuvStandardMatrix::Bt709),
            // Unspecified: default to BT.601 (common practice, matches most encoders)
            2 => Some(yuv::YuvStandardMatrix::Bt601),
            4 => Some(yuv::YuvStandardMatrix::Fcc),
            5 => Some(yuv::YuvStandardMatrix::Bt470_6),
            6 => Some(yuv::YuvStandardMatrix::Bt601),
            7 => Some(yuv::YuvStandardMatrix::Smpte240),
            9 | 10 => Some(yuv::YuvStandardMatrix::Bt2020),
            // Identity (0) and YCgCo (8) handled separately by caller.
            // Other values (11-14) are unsupported → fallback.
            _ => None,
        }
    }

    /// Reads a YUV plane from an `avifImage` as a `&[u8]` slice.
    /// `plane_idx`: 0=Y, 1=U, 2=V.
    ///
    /// # Safety
    /// `image` must be a valid, non-null pointer to a decoded `avifImage`.
    unsafe fn yuv_plane_slice(
        image: *const libavif_sys::avifImage,
        plane_idx: usize,
        row_count: u32,
    ) -> &'static [u8] {
        let ptr = (*image).yuvPlanes[plane_idx];
        let row_bytes = (*image).yuvRowBytes[plane_idx];
        if ptr.is_null() || row_bytes == 0 || row_count == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(ptr, (row_bytes * row_count) as usize)
        }
    }

    /// Reads a 10/12-bit YUV plane from an `avifImage` as a `&[u16]` slice.
    /// libavif stores high-bit-depth planes as u16 LE packed into a `*mut u8` buffer;
    /// `yuvRowBytes` is in bytes, so the u16 stride is `yuvRowBytes / 2`.
    ///
    /// # Safety
    /// `image` must be a valid, non-null pointer to a decoded `avifImage`.
    /// The plane must contain u16 LE data (i.e. `depth` is 10 or 12).
    /// `yuvRowBytes[plane_idx]` must be even (guaranteed by AVIF spec).
    unsafe fn yuv_plane_slice_u16(
        image: *const libavif_sys::avifImage,
        plane_idx: usize,
        row_count: u32,
    ) -> &'static [u16] {
        let ptr = (*image).yuvPlanes[plane_idx];
        let row_bytes = (*image).yuvRowBytes[plane_idx] as usize;
        if ptr.is_null() || row_bytes == 0 || row_count == 0 {
            &[]
        } else {
            let stride_u16 = row_bytes / 2;
            std::slice::from_raw_parts(ptr as *const u16, stride_u16 * row_count as usize)
        }
    }

    /// Attempts SIMD-accelerated YUV→RGBA conversion for 8-bit images.
    /// Returns `Err(())` for unsupported formats (caller should use the fallback).
    ///
    /// # Safety
    /// `image` must be a valid, non-null pointer to a decoded `avifImage`
    /// whose YUV planes are populated (i.e. after `avifDecoderNextImage`).
    fn convert_yuv_to_rgba_simd(image: *const libavif_sys::avifImage) -> Result<Vec<u8>, ()> {
        // SAFETY: caller guarantees image is valid and non-null.
        let (width, height, depth, fmt, range, mc) = unsafe {
            (
                (*image).width,
                (*image).height,
                (*image).depth,
                (*image).yuvFormat,
                (*image).yuvRange,
                (*image).matrixCoefficients,
            )
        };

        if depth != 8 {
            return convert_yuv_to_rgba_simd_p16(image, width, height, depth, fmt, range, mc);
        }

        let yuv_range = map_range(range);
        let rgba_stride = width * 4;
        let mut rgba = vec![0u8; (width * height * 4) as usize];

        // Identity matrix (GBR) — special dispatch, no matrix parameter.
        if mc as u32 == 0 {
            convert_identity(image, width, height, fmt, yuv_range, &mut rgba, rgba_stride)?;
            apply_alpha(image, &mut rgba, width, height);
            return Ok(rgba);
        }

        // YCgCo matrix — special dispatch, no matrix parameter.
        if mc as u32 == 8 {
            convert_ycgco(image, width, height, fmt, yuv_range, &mut rgba, rgba_stride)?;
            apply_alpha(image, &mut rgba, width, height);
            return Ok(rgba);
        }

        // Standard YCbCr matrices.
        let matrix = map_matrix(mc).ok_or(())?;

        // SAFETY: image planes are valid after avifDecoderNextImage.
        unsafe {
            match fmt {
                libavif_sys::AVIF_PIXEL_FORMAT_YUV420 => {
                    let chroma_h = (height + 1) / 2;
                    let planar = yuv::YuvPlanarImage {
                        y_plane: yuv_plane_slice(image, 0, height),
                        y_stride: (*image).yuvRowBytes[0],
                        u_plane: yuv_plane_slice(image, 1, chroma_h),
                        u_stride: (*image).yuvRowBytes[1],
                        v_plane: yuv_plane_slice(image, 2, chroma_h),
                        v_stride: (*image).yuvRowBytes[2],
                        width,
                        height,
                    };
                    yuv::yuv420_to_rgba(&planar, &mut rgba, rgba_stride, yuv_range, matrix)
                        .map_err(|_| ())?;
                }
                libavif_sys::AVIF_PIXEL_FORMAT_YUV422 => {
                    let planar = yuv::YuvPlanarImage {
                        y_plane: yuv_plane_slice(image, 0, height),
                        y_stride: (*image).yuvRowBytes[0],
                        u_plane: yuv_plane_slice(image, 1, height),
                        u_stride: (*image).yuvRowBytes[1],
                        v_plane: yuv_plane_slice(image, 2, height),
                        v_stride: (*image).yuvRowBytes[2],
                        width,
                        height,
                    };
                    yuv::yuv422_to_rgba(&planar, &mut rgba, rgba_stride, yuv_range, matrix)
                        .map_err(|_| ())?;
                }
                libavif_sys::AVIF_PIXEL_FORMAT_YUV444 => {
                    let planar = yuv::YuvPlanarImage {
                        y_plane: yuv_plane_slice(image, 0, height),
                        y_stride: (*image).yuvRowBytes[0],
                        u_plane: yuv_plane_slice(image, 1, height),
                        u_stride: (*image).yuvRowBytes[1],
                        v_plane: yuv_plane_slice(image, 2, height),
                        v_stride: (*image).yuvRowBytes[2],
                        width,
                        height,
                    };
                    yuv::yuv444_to_rgba(&planar, &mut rgba, rgba_stride, yuv_range, matrix)
                        .map_err(|_| ())?;
                }
                libavif_sys::AVIF_PIXEL_FORMAT_YUV400 => {
                    let gray = yuv::YuvGrayImage {
                        y_plane: yuv_plane_slice(image, 0, height),
                        y_stride: (*image).yuvRowBytes[0],
                        width,
                        height,
                    };
                    yuv::yuv400_to_rgba(&gray, &mut rgba, rgba_stride, yuv_range, matrix)
                        .map_err(|_| ())?;
                }
                _ => return Err(()),
            }
        }

        // Apply alpha plane if present.
        apply_alpha(image, &mut rgba, width, height);

        Ok(rgba)
    }

    /// Handles Identity matrix (matrixCoefficients=0, aka GBR) conversion.
    fn convert_identity(
        image: *const libavif_sys::avifImage,
        width: u32,
        height: u32,
        fmt: libavif_sys::avifPixelFormat,
        range: yuv::YuvRange,
        rgba: &mut [u8],
        rgba_stride: u32,
    ) -> Result<(), ()> {
        // Identity/GBR only makes sense with 4:4:4.
        if fmt != libavif_sys::AVIF_PIXEL_FORMAT_YUV444 {
            return Err(());
        }
        unsafe {
            let planar = yuv::YuvPlanarImage {
                y_plane: yuv_plane_slice(image, 0, height),
                y_stride: (*image).yuvRowBytes[0],
                u_plane: yuv_plane_slice(image, 1, height),
                u_stride: (*image).yuvRowBytes[1],
                v_plane: yuv_plane_slice(image, 2, height),
                v_stride: (*image).yuvRowBytes[2],
                width,
                height,
            };
            yuv::gbr_to_rgba(&planar, rgba, rgba_stride, range).map_err(|_| ())?;
        }
        Ok(())
    }

    /// Handles YCgCo matrix (matrixCoefficients=8) conversion.
    fn convert_ycgco(
        image: *const libavif_sys::avifImage,
        width: u32,
        height: u32,
        fmt: libavif_sys::avifPixelFormat,
        range: yuv::YuvRange,
        rgba: &mut [u8],
        rgba_stride: u32,
    ) -> Result<(), ()> {
        unsafe {
            match fmt {
                libavif_sys::AVIF_PIXEL_FORMAT_YUV420 => {
                    let chroma_h = (height + 1) / 2;
                    let planar = yuv::YuvPlanarImage {
                        y_plane: yuv_plane_slice(image, 0, height),
                        y_stride: (*image).yuvRowBytes[0],
                        u_plane: yuv_plane_slice(image, 1, chroma_h),
                        u_stride: (*image).yuvRowBytes[1],
                        v_plane: yuv_plane_slice(image, 2, chroma_h),
                        v_stride: (*image).yuvRowBytes[2],
                        width,
                        height,
                    };
                    yuv::ycgco420_to_rgba(&planar, rgba, rgba_stride, range).map_err(|_| ())?;
                }
                libavif_sys::AVIF_PIXEL_FORMAT_YUV422 => {
                    let planar = yuv::YuvPlanarImage {
                        y_plane: yuv_plane_slice(image, 0, height),
                        y_stride: (*image).yuvRowBytes[0],
                        u_plane: yuv_plane_slice(image, 1, height),
                        u_stride: (*image).yuvRowBytes[1],
                        v_plane: yuv_plane_slice(image, 2, height),
                        v_stride: (*image).yuvRowBytes[2],
                        width,
                        height,
                    };
                    yuv::ycgco422_to_rgba(&planar, rgba, rgba_stride, range).map_err(|_| ())?;
                }
                libavif_sys::AVIF_PIXEL_FORMAT_YUV444 => {
                    let planar = yuv::YuvPlanarImage {
                        y_plane: yuv_plane_slice(image, 0, height),
                        y_stride: (*image).yuvRowBytes[0],
                        u_plane: yuv_plane_slice(image, 1, height),
                        u_stride: (*image).yuvRowBytes[1],
                        v_plane: yuv_plane_slice(image, 2, height),
                        v_stride: (*image).yuvRowBytes[2],
                        width,
                        height,
                    };
                    yuv::ycgco444_to_rgba(&planar, rgba, rgba_stride, range).map_err(|_| ())?;
                }
                _ => return Err(()),
            }
        }
        Ok(())
    }

    /// SIMD-accelerated YUV→RGBA for 10-bit and 12-bit AVIF images.
    /// Returns `Err(())` for formats not covered by the `yuv` crate (caller falls back to libavif).
    ///
    /// Covered:
    ///   - 4:2:0 10-bit (`i010_to_rgba`), 12-bit (`i012_to_rgba`)
    ///   - 4:2:2 10-bit (`i210_to_rgba`), 12-bit (`i212_to_rgba`)
    ///   - 4:4:4 10-bit (`i410_to_rgba`)
    ///
    /// Falls back (returns `Err`): 4:4:4 12-bit, 4:0:0, YCgCo (mc=8), Identity/GBR (mc=0),
    /// and any matrix without precomputed 10/12-bit coefficients in the yuv crate (Bt470_6,
    /// Fcc, Smpte240). The yuv crate's on-the-fly coefficient path for 10/12-bit has a
    /// bug where `range_bgra` receives `BIT_DEPTH` instead of 255, making y_coef ~80x too
    /// small. Only matrices with precomputed entries (Bt601, Bt709, Bt2020) are safe.
    fn convert_yuv_to_rgba_simd_p16(
        image: *const libavif_sys::avifImage,
        width: u32,
        height: u32,
        depth: u32,
        fmt: libavif_sys::avifPixelFormat,
        range: libavif_sys::avifRange,
        mc: u16,
    ) -> Result<Vec<u8>, ()> {
        // YCgCo and Identity/GBR have no p16 decode functions in the yuv crate.
        if mc as u32 == 0 || mc as u32 == 8 {
            return Err(());
        }
        // 4:4:4 12-bit: no i412 in yuv 0.8 — fall back.
        if fmt == libavif_sys::AVIF_PIXEL_FORMAT_YUV444 && depth == 12 {
            return Err(());
        }
        // Monochrome: no yuv-crate path for p16→rgba8 — fall back.
        if fmt == libavif_sys::AVIF_PIXEL_FORMAT_YUV400 {
            return Err(());
        }

        // Only Bt601/Bt709/Bt2020 have precomputed 10/12-bit inverse coefficients.
        // Other matrices (Bt470_6 mc=5, Fcc mc=4, Smpte240 mc=7) trigger a buggy
        // on-the-fly code path in the yuv crate that uses BIT_DEPTH instead of 255
        // as range_bgra, producing y_coef ~80x too small and near-zero output.
        let matrix = match mc as u32 {
            1 => yuv::YuvStandardMatrix::Bt709,
            2 | 6 => yuv::YuvStandardMatrix::Bt601,
            9 | 10 => yuv::YuvStandardMatrix::Bt2020,
            _ => return Err(()),
        };
        let yuv_range = map_range(range);
        let rgba_stride = width * 4;
        let mut rgba = vec![0u8; (width * height * 4) as usize];

        // SAFETY: image planes are valid after avifDecoderNextImage.
        // yuvRowBytes are in bytes; divide by 2 for u16 stride (even by AVIF spec).
        unsafe {
            match (fmt, depth) {
                (libavif_sys::AVIF_PIXEL_FORMAT_YUV420, 10) => {
                    let ch = (height + 1) / 2;
                    let planar = yuv::YuvPlanarImage {
                        y_plane: yuv_plane_slice_u16(image, 0, height),
                        y_stride: (*image).yuvRowBytes[0] / 2,
                        u_plane: yuv_plane_slice_u16(image, 1, ch),
                        u_stride: (*image).yuvRowBytes[1] / 2,
                        v_plane: yuv_plane_slice_u16(image, 2, ch),
                        v_stride: (*image).yuvRowBytes[2] / 2,
                        width,
                        height,
                    };
                    yuv::i010_to_rgba(&planar, &mut rgba, rgba_stride, yuv_range, matrix)
                        .map_err(|_| ())?;
                }
                (libavif_sys::AVIF_PIXEL_FORMAT_YUV420, 12) => {
                    let ch = (height + 1) / 2;
                    let planar = yuv::YuvPlanarImage {
                        y_plane: yuv_plane_slice_u16(image, 0, height),
                        y_stride: (*image).yuvRowBytes[0] / 2,
                        u_plane: yuv_plane_slice_u16(image, 1, ch),
                        u_stride: (*image).yuvRowBytes[1] / 2,
                        v_plane: yuv_plane_slice_u16(image, 2, ch),
                        v_stride: (*image).yuvRowBytes[2] / 2,
                        width,
                        height,
                    };
                    yuv::i012_to_rgba(&planar, &mut rgba, rgba_stride, yuv_range, matrix)
                        .map_err(|_| ())?;
                }
                (libavif_sys::AVIF_PIXEL_FORMAT_YUV422, 10) => {
                    let planar = yuv::YuvPlanarImage {
                        y_plane: yuv_plane_slice_u16(image, 0, height),
                        y_stride: (*image).yuvRowBytes[0] / 2,
                        u_plane: yuv_plane_slice_u16(image, 1, height),
                        u_stride: (*image).yuvRowBytes[1] / 2,
                        v_plane: yuv_plane_slice_u16(image, 2, height),
                        v_stride: (*image).yuvRowBytes[2] / 2,
                        width,
                        height,
                    };
                    yuv::i210_to_rgba(&planar, &mut rgba, rgba_stride, yuv_range, matrix)
                        .map_err(|_| ())?;
                }
                (libavif_sys::AVIF_PIXEL_FORMAT_YUV422, 12) => {
                    let planar = yuv::YuvPlanarImage {
                        y_plane: yuv_plane_slice_u16(image, 0, height),
                        y_stride: (*image).yuvRowBytes[0] / 2,
                        u_plane: yuv_plane_slice_u16(image, 1, height),
                        u_stride: (*image).yuvRowBytes[1] / 2,
                        v_plane: yuv_plane_slice_u16(image, 2, height),
                        v_stride: (*image).yuvRowBytes[2] / 2,
                        width,
                        height,
                    };
                    yuv::i212_to_rgba(&planar, &mut rgba, rgba_stride, yuv_range, matrix)
                        .map_err(|_| ())?;
                }
                (libavif_sys::AVIF_PIXEL_FORMAT_YUV444, 10) => {
                    let planar = yuv::YuvPlanarImage {
                        y_plane: yuv_plane_slice_u16(image, 0, height),
                        y_stride: (*image).yuvRowBytes[0] / 2,
                        u_plane: yuv_plane_slice_u16(image, 1, height),
                        u_stride: (*image).yuvRowBytes[1] / 2,
                        v_plane: yuv_plane_slice_u16(image, 2, height),
                        v_stride: (*image).yuvRowBytes[2] / 2,
                        width,
                        height,
                    };
                    yuv::i410_to_rgba(&planar, &mut rgba, rgba_stride, yuv_range, matrix)
                        .map_err(|_| ())?;
                }
                _ => return Err(()),
            }
        }

        apply_alpha_16bit(image, &mut rgba, width, height, depth);
        Ok(rgba)
    }

    /// Composites a 10/12-bit alpha plane onto an RGBA8 buffer.
    /// Alpha samples are u16 LE; shifted right by `(depth - 8)` to produce u8.
    /// If no alpha plane is present, sets all alpha bytes to 255 (fully opaque).
    fn apply_alpha_16bit(
        image: *const libavif_sys::avifImage,
        rgba: &mut [u8],
        width: u32,
        height: u32,
        depth: u32,
    ) {
        // SAFETY: image is a valid pointer from the decoder.
        let (alpha_ptr, alpha_row_bytes) = unsafe { ((*image).alphaPlane, (*image).alphaRowBytes) };

        if alpha_ptr.is_null() || alpha_row_bytes == 0 {
            for pixel in rgba.chunks_exact_mut(4) {
                pixel[3] = 255;
            }
        } else {
            let shift = depth - 8; // 10-bit → >>2, 12-bit → >>4
            let stride_u16 = (alpha_row_bytes / 2) as usize;
            // SAFETY: alpha_ptr points to u16 LE samples; alpha_row_bytes is even (AVIF spec).
            let alpha_u16 = unsafe {
                std::slice::from_raw_parts(alpha_ptr as *const u16, stride_u16 * height as usize)
            };
            for y in 0..height as usize {
                let alpha_row = y * stride_u16;
                let rgba_row = y * width as usize * 4;
                for x in 0..width as usize {
                    rgba[rgba_row + x * 4 + 3] = (alpha_u16[alpha_row + x] >> shift) as u8;
                }
            }
        }
    }

    /// Composites the alpha plane from an `avifImage` onto an RGBA buffer.
    /// If no alpha plane is present, sets all alpha bytes to 255 (fully opaque).
    fn apply_alpha(image: *const libavif_sys::avifImage, rgba: &mut [u8], width: u32, height: u32) {
        // SAFETY: image is a valid pointer from the decoder.
        let (alpha_ptr, alpha_row_bytes) = unsafe { ((*image).alphaPlane, (*image).alphaRowBytes) };

        if alpha_ptr.is_null() || alpha_row_bytes == 0 {
            // No alpha plane — set all alpha bytes to 255 (opaque).
            for pixel in rgba.chunks_exact_mut(4) {
                pixel[3] = 255;
            }
        } else {
            // SAFETY: alpha_ptr is non-null and points to decoder-owned data.
            let alpha_slice = unsafe {
                std::slice::from_raw_parts(alpha_ptr, (alpha_row_bytes * height) as usize)
            };
            for y in 0..height as usize {
                let alpha_row_start = y * alpha_row_bytes as usize;
                let rgba_row_start = y * (width as usize * 4);
                for x in 0..width as usize {
                    rgba[rgba_row_start + x * 4 + 3] = alpha_slice[alpha_row_start + x];
                }
            }
        }
    }

    struct DecoderGuard {
        decoder: *mut libavif_sys::avifDecoder,
    }

    impl DecoderGuard {
        fn new() -> Option<Self> {
            // SAFETY: libavif_sys::avifDecoderCreate() is safe to call.
            let decoder = unsafe { libavif_sys::avifDecoderCreate() };
            if decoder.is_null() {
                None
            } else {
                // SAFETY: decoder is checked for null before dereferencing.
                unsafe {
                    let num_cpus = std::thread::available_parallelism()
                        .map(|p| p.get() as i32)
                        .unwrap_or(4);
                    (*decoder).maxThreads = num_cpus;
                }
                Some(Self { decoder })
            }
        }
    }

    impl Drop for DecoderGuard {
        fn drop(&mut self) {
            // SAFETY: self.decoder was created by avifDecoderCreate and hasn't been destroyed yet.
            unsafe {
                libavif_sys::avifDecoderDestroy(self.decoder);
            }
        }
    }

    struct RgbImageGuard {
        rgb: libavif_sys::avifRGBImage,
        allocated: bool,
    }

    impl RgbImageGuard {
        fn new() -> Self {
            Self {
                // SAFETY: libavif_sys::avifRGBImage is a POD type and can be safely zero-initialized.
                rgb: unsafe { std::mem::zeroed() },
                allocated: false,
            }
        }

        fn allocate(&mut self, image: *const libavif_sys::avifImage) {
            // SAFETY: self.rgb is zero-initialized and image is a valid pointer from the decoder.
            unsafe {
                libavif_sys::avifRGBImageSetDefaults(&mut self.rgb, image);
                self.rgb.format = libavif_sys::AVIF_RGB_FORMAT_RGBA;
                self.rgb.depth = 8;
                libavif_sys::avifRGBImageAllocatePixels(&mut self.rgb);
                self.allocated = true;
            }
        }

        /// Reuses the existing pixel buffer if the frame dimensions haven't changed,
        /// otherwise frees and reallocates. Avoids repeated malloc/free syscalls
        /// for animations where all frames share the same resolution (the common case).
        fn ensure_allocated(&mut self, image: *const libavif_sys::avifImage) {
            // SAFETY: image is a valid pointer from the decoder.
            let (new_w, new_h) = unsafe { ((*image).width, (*image).height) };

            if self.allocated && self.rgb.width == new_w && self.rgb.height == new_h {
                // Buffer already the correct size — skip reallocation.
                return;
            }

            // Dimensions changed or first frame — (re)allocate.
            if self.allocated {
                // SAFETY: self.rgb.pixels was allocated by avifRGBImageAllocatePixels.
                unsafe {
                    libavif_sys::avifRGBImageFreePixels(&mut self.rgb);
                }
                self.allocated = false;
            }
            self.allocate(image);
        }

        fn convert_from_yuv(&mut self, image: *const libavif_sys::avifImage) {
            // SAFETY: image is a valid pointer from the decoder, and self.rgb has had pixels allocated.
            unsafe {
                libavif_sys::avifImageYUVToRGB(image, &mut self.rgb);
            }
        }

        fn extract_pixels(&self) -> Vec<u8> {
            let width = self.rgb.width;
            let height = self.rgb.height;
            let row_bytes = self.rgb.rowBytes;

            // Defensive: handle invalid dimensions
            if width == 0 || height == 0 {
                return Vec::new();
            }

            // SAFETY: self.rgb.pixels was allocated by avifRGBImageAllocatePixels.
            unsafe {
                // Fast path: data is already contiguous (no row padding)
                // This is true for most AVIF images
                if row_bytes == width * 4 {
                    let total_pixels = (width * height * 4) as usize;
                    std::slice::from_raw_parts(self.rgb.pixels, total_pixels).to_vec()
                } else {
                    // Slow path: copy row by row, handling potential row padding
                    let mut packed_pixels = Vec::with_capacity((width * height * 4) as usize);
                    let pixel_slice =
                        std::slice::from_raw_parts(self.rgb.pixels, (row_bytes * height) as usize);
                    for y in 0..height {
                        let src_offset = (y * row_bytes) as usize;
                        let src_row = &pixel_slice[src_offset..src_offset + (width * 4) as usize];
                        packed_pixels.extend_from_slice(src_row);
                    }
                    packed_pixels
                }
            }
        }
    }

    impl Drop for RgbImageGuard {
        fn drop(&mut self) {
            if self.allocated {
                // SAFETY: self.rgb.pixels was allocated by avifRGBImageAllocatePixels.
                unsafe {
                    libavif_sys::avifRGBImageFreePixels(&mut self.rgb);
                }
            }
        }
    }
}
