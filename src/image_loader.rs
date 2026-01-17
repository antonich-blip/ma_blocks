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

impl AnimationFrame {
    pub fn size_vec2(&self) -> egui::Vec2 {
        let [w, h] = self.image.size;
        egui::vec2(w as f32, h as f32)
    }
}

/// Holds all frames and metadata for a loaded image, supporting both static and animated formats.
pub struct LoadedImage {
    pub frames: Vec<AnimationFrame>,
    pub original_size: egui::Vec2,
    pub has_animation: bool,
}

pub type ImageLoadResult = (std::path::PathBuf, LoadedImage, bool);
pub type ImageLoadResponse = Result<ImageLoadResult, String>;

impl LoadedImage {
    pub fn from_frames(frames: Vec<AnimationFrame>, has_animation: bool) -> Self {
        let original_size = frames
            .first()
            .map(|frame| frame.size_vec2())
            .unwrap_or(egui::vec2(1.0, 1.0));
        Self {
            frames,
            original_size,
            has_animation,
        }
    }
}

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
        for frame in &mut loaded.frames {
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
    }

    Ok(loaded)
}

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

fn decode_avif(bytes: &[u8], first_frame_only: bool) -> Result<LoadedImage, String> {
    avif_support::decode(bytes, first_frame_only)
}

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

fn color_image_from_dynamic(image: DynamicImage) -> egui::ColorImage {
    let rgba = image.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    egui::ColorImage::from_rgba_unmultiplied(size, &rgba.into_raw())
}

fn duration_from_delay(delay: image::Delay) -> Duration {
    let (numer, denom) = delay.numer_denom_ms();
    let denom = denom.max(1);
    let millis = (numer as f32) / (denom as f32);
    Duration::from_secs_f32((millis / 1000.0).max(0.001))
}

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

        let image_count = unsafe { (*decoder.decoder).imageCount as usize };
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

        let limit = if first_frame_only {
            1
        } else {
            MAX_ANIMATION_FRAMES
        };

        while (frame_index as usize) < limit {
            let result = unsafe { libavif_sys::avifDecoderNextImage(decoder.decoder) };
            if result == libavif_sys::AVIF_RESULT_OK {
                let image = unsafe { (*decoder.decoder).image };
                if image.is_null() {
                    frame_index += 1;
                    continue;
                }

                let (width, height) = unsafe { ((*image).width, (*image).height) };
                if width == 0 || height == 0 {
                    frame_index += 1;
                    continue;
                }

                let mut rgb = RgbImageGuard::new();
                rgb.allocate(image);
                rgb.convert_from_yuv(image);
                let pixels = rgb.extract_pixels();

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

                frames.push(AvifFrame {
                    pixels,
                    width,
                    height,
                    duration: duration_secs,
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

        let converted = frames
            .into_iter()
            .map(|frame| {
                let size = [frame.width as usize, frame.height as usize];
                let image = ColorImage::from_rgba_unmultiplied(size, &frame.pixels);
                AnimationFrame {
                    image,
                    duration: sanitize_duration(Duration::from_secs_f64(frame.duration.max(0.0))),
                }
            })
            .collect();

        Ok(LoadedImage::from_frames(converted, image_count > 1))
    }

    struct AvifFrame {
        pixels: Vec<u8>,
        width: u32,
        height: u32,
        duration: f64,
    }

    struct DecoderGuard {
        decoder: *mut libavif_sys::avifDecoder,
    }

    impl DecoderGuard {
        fn new() -> Option<Self> {
            let decoder = unsafe { libavif_sys::avifDecoderCreate() };
            if decoder.is_null() {
                None
            } else {
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
                rgb: unsafe { std::mem::zeroed() },
                allocated: false,
            }
        }

        fn allocate(&mut self, image: *const libavif_sys::avifImage) {
            unsafe {
                libavif_sys::avifRGBImageSetDefaults(&mut self.rgb, image);
                self.rgb.format = libavif_sys::AVIF_RGB_FORMAT_RGBA;
                self.rgb.depth = 8;
                libavif_sys::avifRGBImageAllocatePixels(&mut self.rgb);
                self.allocated = true;
            }
        }

        fn convert_from_yuv(&mut self, image: *const libavif_sys::avifImage) {
            unsafe {
                libavif_sys::avifImageYUVToRGB(image, &mut self.rgb);
            }
        }

        fn extract_pixels(&self) -> Vec<u8> {
            let width = self.rgb.width;
            let height = self.rgb.height;
            let row_bytes = self.rgb.rowBytes;

            let mut packed_pixels = Vec::with_capacity((width * height * 4) as usize);
            unsafe {
                let pixel_slice =
                    std::slice::from_raw_parts(self.rgb.pixels, (row_bytes * height) as usize);
                for y in 0..height {
                    let src_offset = (y * row_bytes) as usize;
                    let src_row = &pixel_slice[src_offset..src_offset + (width * 4) as usize];
                    packed_pixels.extend_from_slice(src_row);
                }
            }
            packed_pixels
        }
    }

    impl Drop for RgbImageGuard {
        fn drop(&mut self) {
            if self.allocated {
                unsafe {
                    libavif_sys::avifRGBImageFreePixels(&mut self.rgb);
                }
            }
        }
    }
}
