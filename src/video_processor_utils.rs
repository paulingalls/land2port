use crate::crop;
use crate::image;
use anyhow::Result;
use std::env;
use usls::{Hbb, Viewer, Y};

/// Helper function to check if debug logging is enabled
pub fn is_debug_enabled() -> bool {
    env::var("RUST_LOG")
        .map(|val| val.to_lowercase() == "debug")
        .unwrap_or(false)
}

/// Debug print function that only prints when RUST_LOG=debug
pub fn debug_println(args: std::fmt::Arguments) {
    if is_debug_enabled() {
        println!("{}", args);
    }
}

/// Processes and displays a crop result
pub fn process_and_display_crop(
    img: &usls::Image,
    crop_result: &crop::CropResult,
    viewer: &mut Viewer,
    headless: bool,
) -> Result<()> {
    let cropped_img = image::create_cropped_image(img, crop_result, img.height() as u32)?;
    if !headless {
        viewer.imshow(&cropped_img)?;
    }
    viewer.write_video_frame(&cropped_img)?;
    Ok(())
}

/// Extracts head detections above the probability threshold from YOLO detection results
pub fn extract_objects_above_threshold<'a>(
    detection: &'a Y,
    object_name: &str,
    object_prob_threshold: f32,
) -> Vec<&'a Hbb> {
    if let Some(hbbs) = detection.hbbs() {
        hbbs.iter()
            .filter(|hbb| {
                // Check confidence threshold
                let meets_threshold = if let Some(confidence) = hbb.confidence() {
                    confidence >= object_prob_threshold
                } else {
                    false
                };

                // Check name matching
                let matches_name = if let Some(name) = hbb.name() {
                    name == object_name
                } else {
                    false
                };

                meets_threshold && matches_name
            })
            .collect()
    } else {
        vec![]
    }
} 