use crate::crop::CropResult;
use anyhow::Result;
use image::{RgbImage, imageops::resize};
use usls::Image;

/// Creates a new image by cropping the input image according to the crop result
///
/// # Arguments
/// * `image` - The input image to crop
/// * `crop_result` - The crop result specifying how to crop the image
/// * `target_width` - The desired width of the output image
///
/// # Returns
/// A new image containing either a single 9:16 crop or two 9:8 crops stacked vertically
pub fn create_cropped_image(
    image: &Image,
    crop_result: &CropResult,
    target_width: u32,
) -> Result<Image> {
    // Get the underlying RgbImage
    let mut rgb_image = image.to_rgb8();

    match crop_result {
        CropResult::Single(crop) => {
            // For a single crop, crop the image to the specified area
            let x = crop.x as u32;
            let y = crop.y as u32;
            let width = crop.width as u32;
            let height = crop.height as u32;

            // Use imageops::crop to get the cropped region
            let cropped = image::imageops::crop(&mut rgb_image, x, y, width, height).to_image();

            // Scale the cropped image to match target width if needed
            let scaled = if cropped.width() != target_width {
                resize(
                    &cropped,
                    target_width,
                    (target_width as f32 * (height as f32 / width as f32)) as u32,
                    image::imageops::FilterType::Lanczos3,
                )
            } else {
                cropped
            };

            // Create a new image with 9:16 aspect ratio and black background
            let output_height = (target_width as f32 * (16.0 / 9.0)) as u32;
            let mut result = RgbImage::new(target_width, output_height);

            // Calculate y offset (1/16 of the height)
            let y_offset = output_height / 16;

            // Overlay the scaled image at the calculated y offset
            image::imageops::overlay(&mut result, &scaled, 0, y_offset as i64);

            // Convert back to usls::Image
            Ok(Image::from(result))
        }
        CropResult::Stacked(crop1, crop2) => {
            // For stacked crops, we need to:
            // 1. Crop both areas
            // 2. Create a new image with the combined height
            // 3. Copy both crops into the new image

            // Crop both areas
            let crop1_img = image::imageops::crop(
                &mut rgb_image,
                crop1.x as u32,
                crop1.y as u32,
                crop1.width as u32,
                crop1.height as u32,
            )
            .to_image();

            let crop2_img = image::imageops::crop(
                &mut rgb_image,
                crop2.x as u32,
                crop2.y as u32,
                crop2.width as u32,
                crop2.height as u32,
            )
            .to_image();

            // Scale both crops to match target width if needed
            let scaled1 = if crop1_img.width() != target_width {
                resize(
                    &crop1_img,
                    target_width,
                    (target_width as f32 * (8.0 / 9.0)) as u32,
                    image::imageops::FilterType::Lanczos3,
                )
            } else {
                crop1_img
            };

            let scaled2 = if crop2_img.width() != target_width {
                resize(
                    &crop2_img,
                    target_width,
                    (target_width as f32 * (8.0 / 9.0)) as u32,
                    image::imageops::FilterType::Lanczos3,
                )
            } else {
                crop2_img
            };

            // Create a new image with the combined height
            let total_height = scaled1.height() + scaled2.height();
            let mut result = RgbImage::new(target_width, total_height);

            // Copy the first crop to the top
            image::imageops::overlay(&mut result, &scaled1, 0, 0);

            // Copy the second crop below the first
            image::imageops::overlay(&mut result, &scaled2, 0, scaled1.height() as i64);

            // Convert back to usls::Image
            Ok(Image::from(result))
        }
    }
}

/// Determines if there is a cut between two images by comparing their similarity
///
/// # Arguments
/// * `image1` - The first image to compare
/// * `image2` - The second image to compare
///
/// # Returns
/// `true` if the similarity is less than 0.5 (indicating a cut), `false` otherwise
pub fn is_cut(image1: &Image, image2: &Image) -> Result<bool> {
    // Convert both images to RgbImage for comparison
    let rgb1 = image1.to_rgb8();
    let rgb2 = image2.to_rgb8();
    
    // Use rgb_image_compare to get the similarity score
    let similarity = image_compare::rgb_hybrid_compare(&rgb1, &rgb2)?;

    println!("similarity: {:?}", similarity.score);
    
    // Return true if similarity is less than 0.4 (indicating a cut)
    Ok(similarity.score < 0.4)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crop::CropArea;
    use usls::Image;

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

        // Create two crop areas
        let crop1 = CropArea::new(0.0, 0.0, 1080.0, 960.0); // 9:8 aspect ratio
        let crop2 = CropArea::new(960.0, 0.0, 1080.0, 960.0); // 9:8 aspect ratio
        let crop_result = CropResult::Stacked(crop1, crop2);

        // Create the cropped image with target width of 1080
        let cropped = create_cropped_image(&image, &crop_result, 1080).unwrap();

        // Verify dimensions
        assert_eq!(cropped.width(), 1080);
        assert_eq!(cropped.height(), 1920); // Combined height of both crops
    }

    #[test]
    fn test_is_cut() {
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
        
        // Identical images should not be considered a cut
        assert!(!is_cut(&image1, &image2).unwrap());
        
        // Create a different image
        let mut rgb_image3 = RgbImage::new(100, 100);
        for y in 0..100 {
            for x in 0..100 {
                let pixel = image::Rgb([255 - x as u8, 255 - y as u8, 128]);
                rgb_image3.put_pixel(x, y, pixel);
            }
        }
        
        let image3 = Image::from(rgb_image3);
        
        // Different images should be considered a cut
        assert!(is_cut(&image1, &image3).unwrap());
    }
}
