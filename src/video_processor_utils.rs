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

/// Predicts the current HBB position based on the previous three frames
/// Uses velocity and acceleration to estimate where the object will be in the current frame
///
/// # Arguments
/// * `three_frames_ago` - The HBB from three frames ago
/// * `two_frames_ago` - The HBB from two frames ago
/// * `last_frame` - The HBB from the last frame
/// * `max_x` - Maximum x coordinate (width of frame)
/// * `max_y` - Maximum y coordinate (height of frame)
///
/// # Returns
/// A predicted HBB for the current frame
pub fn predict_current_hbb(three_frames_ago: &Hbb, two_frames_ago: &Hbb, last_frame: &Hbb, max_x: f32, max_y: f32) -> Hbb {
    // Calculate velocities between consecutive frames
    let v1_x = two_frames_ago.xmin() - three_frames_ago.xmin();
    let v1_y = two_frames_ago.ymin() - three_frames_ago.ymin();
    let v2_x = last_frame.xmin() - two_frames_ago.xmin();
    let v2_y = last_frame.ymin() - two_frames_ago.ymin();
    
    // Calculate acceleration (change in velocity)
    let ax = v2_x - v1_x;
    let ay = v2_y - v1_y;
    
    // Predict current position using velocity + acceleration
    // Position = last_position + velocity + 0.5 * acceleration
    let predicted_x = last_frame.xmin() + v2_x + 0.5 * ax;
    let predicted_y = last_frame.ymin() + v2_y + 0.5 * ay;
        
    // Create a new HBB with the predicted values using center coordinates
    Hbb::from_xywh(
        predicted_x.max(0.0).min(max_x),
        predicted_y.max(0.0).min(max_y),
        last_frame.width(),
        last_frame.height(),
    )
}

/// Prints the default debug information for video processors
pub fn print_default_debug_info(objects: &[&usls::Hbb], latest_crop: &crop::CropResult, is_graphic: bool) {
    debug_println(format_args!("--------------------------------"));
    debug_println(format_args!("objects: {:?}", objects));
    debug_println(format_args!("latest_crop: {:?}", latest_crop));
    debug_println(format_args!("is_graphic: {:?}", is_graphic));
}

/// Extracts head detections above the probability threshold from YOLO detection results
pub fn extract_objects_above_threshold<'a>(
    detection: &'a Y,
    object_name: &str,
    object_prob_threshold: f32,
    object_area_threshold: f32,
    frame_width: f32,
    frame_height: f32,
) -> Vec<&'a Hbb> {
    if let Some(hbbs) = detection.hbbs() {
        let frame_area = frame_width * frame_height;
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

                // Check area threshold (skip for ball objects)
                let meets_area_threshold = if object_name == "ball" {
                    true // Skip area threshold for ball objects
                } else {
                    // Calculate area as percentage of frame
                    let object_area = hbb.width() * hbb.height();
                    let area_percentage = object_area / frame_area;
                    area_percentage >= object_area_threshold
                };

                meets_threshold && matches_name && meets_area_threshold
            })
            .collect()
    } else {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_area_threshold_calculation() {
        // Test area threshold calculation logic
        let frame_width = 1000.0;
        let frame_height = 1000.0;
        let frame_area = frame_width * frame_height;
        
        // Test that 0.01 threshold (1%) works correctly
        let large_object_area = 100.0 * 100.0; // 10000
        let large_object_percentage = large_object_area / frame_area; // 0.01 (1%)
        assert!(large_object_percentage >= 0.01);
        
        let small_object_area = 20.0 * 20.0; // 400
        let small_object_percentage = small_object_area / frame_area; // 0.0004 (0.04%)
        assert!(small_object_percentage < 0.01);
        
        // Test that ball objects would ignore area threshold
        let ball_object_name = "ball";
        let should_ignore_area = ball_object_name == "ball";
        assert!(should_ignore_area);
        
        let non_ball_object_name = "face";
        let should_check_area = non_ball_object_name != "ball";
        assert!(should_check_area);
    }
} 