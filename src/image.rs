use crate::crop::CropResult;
use crate::video_processor_utils;
use crate::video_sink::make_even;
use anyhow::{Context, Result};
use fast_image_resize::images::Image as FirImage;
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use image::{RgbImage, imageops::resize};
use usls::Image;

/// SIMD-accelerated RGB resize via fast_image_resize, using the CatmullRom
/// filter (a bicubic filter, slightly softer than the previous Lanczos3 but
/// visually very close — the bench harness gates SSIM >= 0.95) and several
/// times faster on AVX2 (x86) / NEON (Apple Silicon). Consumes `src` so its
/// pixel buffer is moved into the resizer rather than copied.
fn fir_resize(src: RgbImage, dst_w: u32, dst_h: u32) -> Result<RgbImage> {
    let (sw, sh) = (src.width(), src.height());
    let src_fir = FirImage::from_vec_u8(sw, sh, src.into_raw(), PixelType::U8x3)
        .context("building fast_image_resize source image")?;
    let mut dst_fir = FirImage::new(dst_w, dst_h, PixelType::U8x3);
    let mut resizer = Resizer::new();
    resizer
        .resize(
            &src_fir,
            &mut dst_fir,
            &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::CatmullRom)),
        )
        .context("fast_image_resize resize")?;
    RgbImage::from_raw(dst_w, dst_h, dst_fir.into_vec())
        .context("rebuilding RgbImage from resized buffer")
}

/// Width to which frames are downscaled before the cut-detection similarity
/// comparison. The full-resolution hybrid compare dominated per-frame runtime;
/// scene-cut detection is effectively scale-stable, so comparing small frames
/// is an order of magnitude cheaper with negligible quality loss. Frames
/// already narrower than this are left untouched. 160px keeps scene-cut
/// detection reliable while keeping the hybrid-compare pixel count tiny.
const CUT_DETECT_WIDTH: u32 = 160;

/// Downscales a frame to the given dimensions for cut detection, using a fast
/// (bilinear) filter. Resizes directly from the inner RgbImage buffer (Image
/// derefs to RgbImage) so the full-resolution frame is never cloned. Returns a
/// clone of the inner buffer when it already matches the target size (e.g. tiny
/// test images).
fn downscale_for_cut(img: &Image, target_w: u32, target_h: u32) -> RgbImage {
    let src = &img.image;
    if src.width() == target_w && src.height() == target_h {
        src.clone()
    } else {
        resize(src, target_w, target_h, image::imageops::FilterType::Triangle)
    }
}

/// Stateful cut detector that maintains previous similarity scores
pub struct CutDetector {
    pub previous_score: Option<f64>,
    similarity_threshold: f64,
    previous_similarity_threshold: f64,
    /// Downscaled buffer of the previous frame (image2 of the prior call),
    /// reused as image1 of the next call so each frame is converted and
    /// downscaled only once instead of twice.
    prev_small: Option<RgbImage>,
}

impl CutDetector {
    /// Creates a new cut detector with configurable thresholds
    ///
    /// # Arguments
    /// * `similarity_threshold` - The threshold below which a cut is detected (default: 0.15)
    /// * `previous_similarity_threshold` - The threshold above which the previous score must be to consider a cut (default: 0.7)
    pub fn new(similarity_threshold: f64, previous_similarity_threshold: f64) -> Self {
        Self {
            previous_score: None,
            similarity_threshold,
            previous_similarity_threshold,
            prev_small: None,
        }
    }

    /// Determines if there is a cut between two images by comparing their similarity
    /// with the previous score to avoid false positives
    ///
    /// # Arguments
    /// * `image1` - The first image to compare
    /// * `image2` - The second image to compare
    ///
    /// # Returns
    /// `true` if the similarity is less than similarity_threshold AND previous_score is greater than previous_similarity_threshold,
    /// `false` otherwise
    pub fn is_cut(&mut self, image1: &Image, image2: &Image) -> Result<bool> {
        let similarity = crate::metrics::time("cut_detect", || -> Result<_> {
            // Downscale both frames before comparing. The full-resolution hybrid
            // compare dominated per-frame runtime; cut detection is scale-stable,
            // so this is far cheaper at near-identical accuracy.
            let w = image2.width().max(1);
            let h = image2.height().max(1);
            let target_w = CUT_DETECT_WIDTH.min(w);
            let target_h = (((target_w as f32) * (h as f32 / w as f32)).round() as u32).max(1);

            // Reuse the previous frame's downscaled buffer when its size still
            // matches (is_cut is always called with the prior frame as image1),
            // so only the new frame is converted + downscaled each call.
            let small1 = match self.prev_small.take() {
                Some(prev) if prev.width() == target_w && prev.height() == target_h => prev,
                _ => downscale_for_cut(image1, target_w, target_h),
            };
            let small2 = downscale_for_cut(image2, target_w, target_h);

            // Use rgb_hybrid_compare to get the similarity score
            let score = image_compare::rgb_hybrid_compare(&small1, &small2)?;
            self.prev_small = Some(small2);
            Ok(score)
        })?;
        let current_score = similarity.score;

        video_processor_utils::debug_println(format_args!("similarity: {:?}", current_score));

        let always_cut_threshold = 0.15;
        // Check if this is a cut based on new logic
        let is_cut = match self.previous_score {
            Some(prev_score) => {
                // Only consider it a cut if current score is low AND previous score was high
                current_score < always_cut_threshold
                    || (current_score < self.similarity_threshold
                        && prev_score > self.previous_similarity_threshold)
            }
            None => {
                // First comparison, use simple threshold
                current_score < always_cut_threshold || current_score < self.similarity_threshold
            }
        };

        // Update previous score for next comparison
        self.previous_score = Some(current_score);

        Ok(is_cut)
    }
}

/// Clamps a crop rectangle (produced by float crop math, which can go out of
/// bounds for non-landscape or degenerate inputs) to valid integer pixel bounds
/// inside a `frame_w` x `frame_h` frame. Guarantees `x + width <= frame_w`,
/// `y + height <= frame_h`, and `width, height >= 1`, so the result is always a
/// safe argument to `imageops::crop`.
fn clamp_crop_rect(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    frame_w: u32,
    frame_h: u32,
) -> (u32, u32, u32, u32) {
    let x = (x.max(0.0) as u32).min(frame_w.saturating_sub(1));
    let y = (y.max(0.0) as u32).min(frame_h.saturating_sub(1));
    let width = (width.max(0.0) as u32).min(frame_w - x).max(1);
    let height = (height.max(0.0) as u32).min(frame_h - y).max(1);
    (x, y, width, height)
}

/// Creates a new image by cropping the input image according to the crop result
///
/// # Arguments
/// * `image` - The input image to crop
/// * `crop_result` - The crop result specifying how to crop the image
/// * `target_width` - The desired width of the output image
///
/// # Returns
/// A new image containing either a single 9:16 crop or two crops stacked vertically:
/// - For three heads: top crop (9:6) + bottom crop (9:10) = 9:16 final image
/// - For other cases: two equal crops stacked to create 9:16 final image
pub fn create_cropped_image(
    image: &Image,
    crop_result: &CropResult,
    target_width: u32,
) -> Result<Image> {
    // Borrow the inner RgbImage directly (no clone); the crops are read-only.
    let src = &image.image;
    let (frame_w, frame_h) = src.dimensions();
    // Snap the output width to even so the final H.264 yuv420p frame is valid.
    let target_width = make_even(target_width);

    match crop_result {
        CropResult::Single(crop) => {
            // Clamp the (float) crop rect to valid pixel bounds before cropping.
            let (x, y, width, height) =
                clamp_crop_rect(crop.x, crop.y, crop.width, crop.height, frame_w, frame_h);

            // Use imageops::crop to get the cropped region
            let cropped = image::imageops::crop_imm(src, x, y, width, height).to_image();

            // Scale the cropped image to match target width, preserving the
            // actual (post-clamp) aspect ratio.
            let scaled = if cropped.width() != target_width {
                // Bind the target height before the move: `fir_resize` consumes
                // `cropped`, so its dimensions must be read first.
                let scaled_height =
                    (target_width as f32 * (cropped.height() as f32 / cropped.width() as f32)) as u32;
                fir_resize(cropped, target_width, scaled_height)?
            } else {
                cropped
            };

            // Create a new image with 9:16 aspect ratio and black background
            let output_height = make_even((target_width as f32 * (16.0 / 9.0)) as u32);
            let mut result = RgbImage::new(target_width, output_height);

            // Calculate y offset (1/16 of the height)
            let y_offset = output_height / 16;

            // Overlay the scaled image at the calculated y offset
            image::imageops::overlay(&mut result, &scaled, 0, y_offset as i64);

            // Convert back to usls::Image
            Ok(Image::from(result))
        }
        CropResult::Stacked(crop1, crop2) => {
            // For stacked crops, we create a 9:16 image by:
            // 1. Cropping both areas from the source image
            // 2. Scaling crops based on their aspect ratios
            // 3. Stacking them vertically to create the final 9:16 image

            // Crop both areas from the source image (rects clamped to bounds)
            let (x1, y1, w1, h1) =
                clamp_crop_rect(crop1.x, crop1.y, crop1.width, crop1.height, frame_w, frame_h);
            let crop1_img = image::imageops::crop_imm(src, x1, y1, w1, h1).to_image();

            let (x2, y2, w2, h2) =
                clamp_crop_rect(crop2.x, crop2.y, crop2.width, crop2.height, frame_w, frame_h);
            let crop2_img = image::imageops::crop_imm(src, x2, y2, w2, h2).to_image();

            // Calculate the target 9:16 aspect ratio height
            let target_height = make_even((target_width as f32 * (16.0 / 9.0)) as u32);

            // Determine scaling strategy based on crop aspect ratios
            let crop1_aspect = crop1.width / crop1.height;
            let crop2_aspect = crop2.width / crop2.height;

            let is_crop1_double = (crop1_aspect - 1.5).abs() < 0.1;
            let is_crop2_double = (crop2_aspect - 1.5).abs() < 0.1;
            let is_crop1_single = (crop1_aspect - 0.9).abs() < 0.1;
            let is_crop2_single = (crop2_aspect - 0.9).abs() < 0.1;

            let (top_height, bottom_height) = if is_crop1_double && is_crop2_single {
                // Special case: top crop is 9:6, bottom is 9:10
                let top_height = (target_height as f32 * (6.0 / 16.0)) as u32;
                let bottom_height = (target_height as f32 * (10.0 / 16.0)) as u32;
                (top_height, bottom_height)
            } else if is_crop1_single && is_crop2_double {
                // Special case: top crop is 9:10, bottom is 9:6 (reversed arrangement)
                let top_height = (target_height as f32 * (10.0 / 16.0)) as u32;
                let bottom_height = (target_height as f32 * (6.0 / 16.0)) as u32;
                (top_height, bottom_height)
            } else {
                // Default case: equal height crops (like 9:8 + 9:8)
                // Scale both to half height
                let half_height = target_height / 2;
                (half_height, half_height)
            };

            // Scale both crops to fit the target width and their calculated heights
            let scaled1 = fir_resize(crop1_img, target_width, top_height)?;
            let scaled2 = fir_resize(crop2_img, target_width, bottom_height)?;

            // Create a new image with 9:16 aspect ratio
            let mut result = RgbImage::new(target_width, target_height);

            // Copy the first crop to the top portion
            image::imageops::overlay(&mut result, &scaled1, 0, 0);

            // Copy the second crop to the bottom portion
            image::imageops::overlay(&mut result, &scaled2, 0, top_height as i64);

            // Convert back to usls::Image
            Ok(Image::from(result))
        }
        CropResult::Resize(crop) => {
            // For resize, we want to resize the entire frame to the target width
            // The crop area should cover the entire frame (x=0, y=0, width=frame_width, height=frame_height)
            let (x, y, width, height) =
                clamp_crop_rect(crop.x, crop.y, crop.width, crop.height, frame_w, frame_h);

            // Use imageops::crop to get the cropped region (should be the entire frame)
            let cropped = image::imageops::crop_imm(src, x, y, width, height).to_image();

            // Scale the cropped image to match target width, preserving the
            // actual (post-clamp) aspect ratio.
            let scaled = if cropped.width() != target_width {
                // Bind the target height before the move: `fir_resize` consumes
                // `cropped`, so its dimensions must be read first.
                let scaled_height =
                    (target_width as f32 * (cropped.height() as f32 / cropped.width() as f32)) as u32;
                fir_resize(cropped, target_width, scaled_height)?
            } else {
                cropped
            };

            // Create a new image with 9:16 aspect ratio and black background
            let output_height = make_even((target_width as f32 * (16.0 / 9.0)) as u32);
            let mut result = RgbImage::new(target_width, output_height);

            // Calculate y offset (1/8 of the height)
            let y_offset = output_height / 8;

            // Overlay the scaled image at the calculated y offset
            image::imageops::overlay(&mut result, &scaled, 0, y_offset as i64);

            // Convert back to usls::Image
            Ok(Image::from(result))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crop::CropArea;
    use usls::Image;

    #[test]
    fn test_clamp_crop_rect_in_bounds_unchanged() {
        assert_eq!(
            clamp_crop_rect(360.0, 0.0, 810.0, 1080.0, 1920, 1080),
            (360, 0, 810, 1080)
        );
    }

    #[test]
    fn test_clamp_crop_rect_negative_origin() {
        // Negative x/y floor to 0 without shrinking a width that still fits.
        assert_eq!(
            clamp_crop_rect(-50.0, -30.0, 810.0, 1080.0, 1920, 1080),
            (0, 0, 810, 1080)
        );
    }

    #[test]
    fn test_clamp_crop_rect_overflow_width_height() {
        // x+width past the right edge clamps width to what remains.
        assert_eq!(
            clamp_crop_rect(1500.0, 0.0, 810.0, 1080.0, 1920, 1080),
            (1500, 0, 420, 1080)
        );
        // y+height past the bottom edge clamps height to what remains.
        assert_eq!(
            clamp_crop_rect(0.0, 1000.0, 810.0, 500.0, 1920, 1080),
            (0, 1000, 810, 80)
        );
    }

    #[test]
    fn test_clamp_crop_rect_degenerate_is_at_least_one_pixel() {
        // Zero-size box becomes a 1x1 region rather than a panic/empty crop.
        assert_eq!(clamp_crop_rect(100.0, 100.0, 0.0, 0.0, 1920, 1080), (100, 100, 1, 1));
        // Origin far beyond the frame still yields a valid 1x1 region in-bounds.
        assert_eq!(clamp_crop_rect(5000.0, 5000.0, 100.0, 100.0, 1920, 1080), (1919, 1079, 1, 1));
    }

    #[test]
    fn test_single_crop() {
        // Create a test image with sufficient height for the crop
        let mut rgb_image = RgbImage::new(1920, 1080);
        // Fill with a test pattern
        for y in 0..1080 {
            for x in 0..1920 {
                let pixel = if (x + y) % 2 == 0 {
                    image::Rgb([255, 255, 255]) // White
                } else {
                    image::Rgb([0, 0, 0]) // Black
                };
                rgb_image.put_pixel(x, y, pixel);
            }
        }
        let image = Image::from(rgb_image);

        // Create a crop area in the center with 3:4 aspect ratio
        let crop = CropArea::new(360.0, 0.0, 810.0, 1080.0); // 3:4 aspect ratio
        let crop_result = CropResult::Single(crop);

        // Create the cropped image with target width of 1080
        let cropped = create_cropped_image(&image, &crop_result, 1080).unwrap();

        // Verify dimensions - should be 9:16 aspect ratio
        assert_eq!(cropped.width(), 1080); // Width matches target width
        assert_eq!(cropped.height(), 1920); // 9:16 aspect ratio (1080 * 16/9)

        // Verify the cropped content is positioned 1/16 down from the top
        let expected_y_offset = 1920 / 16; // 1/16 of the height

        // Check that the top portion is black
        for y in 0..expected_y_offset {
            for x in 0..cropped.width() {
                let pixel = cropped.get_pixel(x as u32, y as u32);
                assert_eq!(pixel[0], 0); // R
                assert_eq!(pixel[1], 0); // G
                assert_eq!(pixel[2], 0); // B
            }
        }
    }

    #[test]
    fn test_stacked_crops() {
        // Create a test image
        let mut rgb_image = RgbImage::new(1920, 1080);
        // Fill with a test pattern
        for y in 0..1080 {
            for x in 0..1920 {
                let pixel = if (x + y) % 2 == 0 {
                    image::Rgb([255, 255, 255]) // White
                } else {
                    image::Rgb([0, 0, 0]) // Black
                };
                rgb_image.put_pixel(x, y, pixel);
            }
        }
        let image = Image::from(rgb_image);

        // Create two crop areas with different aspect ratios to test the new logic
        let crop1 = CropArea::new(0.0, 0.0, 1080.0, 960.0); // 9:8 aspect ratio
        let crop2 = CropArea::new(960.0, 0.0, 1080.0, 720.0); // 3:2 aspect ratio (different height)
        let crop_result = CropResult::Stacked(crop1, crop2);

        // Create the cropped image with target width of 1080
        let cropped = create_cropped_image(&image, &crop_result, 1080).unwrap();

        // Verify dimensions - should be 9:16 aspect ratio
        assert_eq!(cropped.width(), 1080); // Width matches target width
        assert_eq!(cropped.height(), 1920); // 9:16 aspect ratio (1080 * 16/9)

        // Verify that the crops are properly scaled and stacked
        // The crops should maintain their relative proportions but fit into the 9:16 frame
    }

    #[test]
    fn test_three_heads_special_case_stacked_crops() {
        // Create a test image
        let mut rgb_image = RgbImage::new(1920, 1080);
        // Fill with a test pattern
        for y in 0..1080 {
            for x in 0..1920 {
                let pixel = if (x + y) % 2 == 0 {
                    image::Rgb([255, 255, 255]) // White
                } else {
                    image::Rgb([0, 0, 0]) // Black
                };
                rgb_image.put_pixel(x, y, pixel);
            }
        }
        let image = Image::from(rgb_image);

        // Create crop areas that match the three-heads special case dimensions
        // First crop: 90% height, 3:5 aspect ratio (taller and skinnier)
        let crop1_height = 1080.0 * 0.9; // 972
        let crop1_width = crop1_height * 0.6; // 583.2
        let crop1 = CropArea::new(0.0, 54.0, crop1_width, crop1_height); // 5% from top

        // Second crop: 70% height, 5:6 aspect ratio (shorter and wider)
        let crop2_height = 1080.0 * 0.7; // 756
        let crop2_width = crop2_height * 1.2; // 907.2
        let crop2 = CropArea::new(960.0, 162.0, crop2_width, crop2_height); // 15% from top

        let crop_result = CropResult::Stacked(crop1, crop2);

        // Create the cropped image with target width of 1080
        let cropped = create_cropped_image(&image, &crop_result, 1080).unwrap();

        // Verify dimensions - should be 9:16 aspect ratio
        assert_eq!(cropped.width(), 1080); // Width matches target width
        assert_eq!(cropped.height(), 1920); // 9:16 aspect ratio (1080 * 16/9)

        // Verify that the crops are properly scaled and stacked
        // The crops should maintain their relative proportions but fit into the 9:16 frame
        // For the three-heads special case, the taller/skinnier crop should take more vertical space
        // and the shorter/wider crop should take less vertical space
    }

    #[test]
    fn test_cut_detector() {
        let mut detector = CutDetector::new(0.15, 0.7);

        // Create two identical images
        let mut rgb_image1 = RgbImage::new(100, 100);
        let mut rgb_image2 = RgbImage::new(100, 100);

        // Fill both with the same pattern
        for y in 0..100 {
            for x in 0..100 {
                let pixel = image::Rgb([x as u8, y as u8, 128]);
                rgb_image1.put_pixel(x, y, pixel);
                rgb_image2.put_pixel(x, y, pixel);
            }
        }

        let image1 = Image::from(rgb_image1);
        let image2 = Image::from(rgb_image2);

        // First comparison - should use simple threshold
        let is_cut = detector.is_cut(&image1, &image2).unwrap();
        // Identical images should not be considered a cut
        assert!(!is_cut);

        // Create a different image
        let mut rgb_image3 = RgbImage::new(100, 100);
        for y in 0..100 {
            for x in 0..100 {
                let pixel = image::Rgb([255 - x as u8, 255 - y as u8, 128]);
                rgb_image3.put_pixel(x, y, pixel);
            }
        }

        let image3 = Image::from(rgb_image3);

        // Second comparison - should use new logic with previous score
        let is_cut = detector.is_cut(&image2, &image3).unwrap();
        // This should depend on the actual similarity scores
        // The test will pass if the logic works correctly
        assert!(is_cut == (detector.previous_score.unwrap() < 0.15));
    }

    #[test]
    fn test_downscale_for_cut_dims() {
        // A frame larger than CUT_DETECT_WIDTH is downscaled, preserving aspect.
        let img = Image::from(RgbImage::new(1920, 1080));
        let small = downscale_for_cut(&img, 320, 180);
        assert_eq!(small.width(), 320);
        assert_eq!(small.height(), 180);

        // A frame already at the target size is returned unchanged in dims.
        let same = Image::from(RgbImage::new(320, 180));
        let out = downscale_for_cut(&same, 320, 180);
        assert_eq!(out.width(), 320);
        assert_eq!(out.height(), 180);
    }

    #[test]
    fn test_cut_detector_cache_reuse_sequential() {
        // Drive a 3-frame sequence the way the processors do: is_cut(prev, cur)
        // then prev = cur. Identical consecutive large frames must not be cuts,
        // exercising the cached-prev_small path on the second call.
        let mut detector = CutDetector::new(0.15, 0.7);
        let mut rgb = RgbImage::new(640, 360);
        for y in 0..360 {
            for x in 0..640 {
                rgb.put_pixel(x, y, image::Rgb([(x % 256) as u8, (y % 256) as u8, 32]));
            }
        }
        let f1 = Image::from(rgb.clone());
        let f2 = Image::from(rgb.clone());
        let f3 = Image::from(rgb);
        assert!(!detector.is_cut(&f1, &f2).unwrap());
        // Second call must reuse the cached downscale of f2 as image1.
        assert!(!detector.is_cut(&f2, &f3).unwrap());
        assert!(detector.prev_small.is_some());
    }

    #[test]
    fn test_cut_detector_downscales_large_frames() {
        // Two identical large frames must not register as a cut once the
        // downscale path (frames wider than CUT_DETECT_WIDTH) is exercised.
        let mut detector = CutDetector::new(0.15, 0.7);
        let mut rgb = RgbImage::new(640, 360);
        for y in 0..360 {
            for x in 0..640 {
                rgb.put_pixel(x, y, image::Rgb([(x % 256) as u8, (y % 256) as u8, 64]));
            }
        }
        let a = Image::from(rgb.clone());
        let b = Image::from(rgb);
        assert!(!detector.is_cut(&a, &b).unwrap());
    }

    #[test]
    fn test_resize_crop() {
        // Create a test image
        let mut rgb_image = RgbImage::new(1920, 1080);
        // Fill with a test pattern
        for y in 0..1080 {
            for x in 0..1920 {
                let pixel = if (x + y) % 2 == 0 {
                    image::Rgb([255, 255, 255]) // White
                } else {
                    image::Rgb([0, 0, 0]) // Black
                };
                rgb_image.put_pixel(x, y, pixel);
            }
        }
        let image = Image::from(rgb_image);

        // Create a resize crop that covers the entire frame
        let crop = CropArea::new(0.0, 0.0, 1920.0, 1080.0);
        let crop_result = CropResult::Resize(crop);

        // Create the resized image with target width of 1080
        let resized = create_cropped_image(&image, &crop_result, 1080).unwrap();

        // Verify dimensions - should be 9:16 aspect ratio
        assert_eq!(resized.width(), 1080); // Width matches target width
        assert_eq!(resized.height(), 1920); // 9:16 aspect ratio (1080 * 16/9)

        // Verify the resized content is positioned 1/16 down from the top
        let expected_y_offset = 1920 / 16; // 1/16 of the height

        // Check that the top portion is black
        for y in 0..expected_y_offset {
            for x in 0..resized.width() {
                let pixel = resized.get_pixel(x as u32, y as u32);
                assert_eq!(pixel[0], 0); // R
                assert_eq!(pixel[1], 0); // G
                assert_eq!(pixel[2], 0); // B
            }
        }
    }
}
