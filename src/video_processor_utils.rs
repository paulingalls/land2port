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

/// Calculates the total area covered by a collection of HBBs
pub fn combined_hbb_area<'a, I>(hbbs: I) -> f32
where
    I: IntoIterator<Item = &'a Hbb>,
{
    const MIN_CONFIDENCE: f32 = 0.80;

    hbbs.into_iter()
        .filter(|hbb| {
            hbb.confidence()
                .map(|conf| conf >= MIN_CONFIDENCE)
                .unwrap_or(false)
        })
        .map(|hbb| hbb.width() * hbb.height())
        .sum()
}

/// Determines whether the combined HBB area exceeds the configured graphic threshold
pub fn is_graphic_area_above_threshold<'a, I>(
    hbbs: I,
    frame_width: f32,
    frame_height: f32,
    graphic_threshold: f32,
) -> bool
where
    I: IntoIterator<Item = &'a Hbb>,
{
    if graphic_threshold <= 0.0 {
        return false;
    }

    let frame_area = frame_width * frame_height;
    if frame_area <= 0.0 {
        return false;
    }

    let total_area = combined_hbb_area(hbbs);
    debug_println(format_args!(
        "total_area: {} >= frame_area * graphic_threshold: {}",
        total_area,
        frame_area * graphic_threshold
    ));
    total_area >= frame_area * graphic_threshold
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
pub fn predict_current_hbb(
    three_frames_ago: &Hbb,
    two_frames_ago: &Hbb,
    last_frame: &Hbb,
    max_x: f32,
    max_y: f32,
) -> Hbb {
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
pub fn print_default_debug_info(
    objects: &[&usls::Hbb],
    latest_crop: &crop::CropResult,
    is_graphic: bool,
) {
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

/// Interpolates between two CropResults over a specified number of frames
///
/// # Arguments
/// * `start` - The starting CropResult
/// * `destination` - The destination CropResult
/// * `num_frames` - Number of frames to interpolate over
///
/// # Returns
/// A vector of CropResults that smoothly transitions from start to destination.
/// If both inputs are Single, it performs linear interpolation on the x-coordinate only,
/// while keeping the y, width, and height from the destination crop.
/// If either input is not Single, it returns a vector filled with the destination CropResult.
pub fn interpolate_crop_results(
    start: &crop::CropResult,
    destination: &crop::CropResult,
    num_frames: usize,
) -> Vec<crop::CropResult> {
    // If either crop result is not Single, return all destination crops
    let (crop::CropResult::Single(start_crop), crop::CropResult::Single(dest_crop)) =
        (start, destination)
    else {
        return vec![destination.clone(); num_frames];
    };

    // Handle edge case of zero or one frame
    if num_frames == 0 {
        return vec![];
    }
    if num_frames == 1 {
        return vec![destination.clone()];
    }

    // Calculate interpolation step
    let step = 1.0 / (num_frames - 1) as f32;

    (0..num_frames)
        .map(|i| {
            let t = i as f32 * step;

            // Only interpolate x-coordinate, keep y, width, and height from destination
            let x = start_crop.x + t * (dest_crop.x - start_crop.x);

            crop::CropResult::Single(crop::CropArea::new(
                x,
                dest_crop.y,
                dest_crop.width,
                dest_crop.height,
            ))
        })
        .collect()
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

    #[test]
    fn test_combined_hbb_area() {
        use super::combined_hbb_area;
        use usls::Hbb;

        let hbbs = vec![
            Hbb::from_xywh(0.0, 0.0, 100.0, 50.0).with_confidence(0.95),
            Hbb::from_xywh(10.0, 10.0, 25.0, 25.0).with_confidence(0.9),
            Hbb::from_xywh(20.0, 20.0, 10.0, 10.0).with_confidence(0.5),
        ];

        let total_area = combined_hbb_area(hbbs.iter());
        let expected_area = 100.0 * 50.0 + 25.0 * 25.0; // low-confidence box excluded

        assert!((total_area - expected_area).abs() < 1e-3);
    }

    #[test]
    fn test_is_graphic_area_above_threshold() {
        use super::is_graphic_area_above_threshold;
        use usls::Hbb;

        let hbbs = vec![
            Hbb::from_xywh(0.0, 0.0, 100.0, 100.0).with_confidence(0.95),
            Hbb::from_xywh(10.0, 10.0, 20.0, 20.0).with_confidence(0.7),
        ];
        let frame_width = 200.0;
        let frame_height = 200.0;

        assert!(is_graphic_area_above_threshold(
            hbbs.iter(),
            frame_width,
            frame_height,
            0.2,
        ));

        assert!(!is_graphic_area_above_threshold(
            hbbs.iter(),
            frame_width,
            frame_height,
            0.3,
        ));
    }

    #[test]
    fn test_interpolate_crop_results_single_to_single() {
        use super::interpolate_crop_results;
        use crate::crop::{CropArea, CropResult};

        let start = CropResult::Single(CropArea::new(100.0, 200.0, 300.0, 400.0));
        let destination = CropResult::Single(CropArea::new(200.0, 300.0, 400.0, 500.0));
        let num_frames = 5;

        let result = interpolate_crop_results(&start, &destination, num_frames);

        assert_eq!(result.len(), num_frames);

        // Check first frame (should have start x, destination y/width/height)
        match &result[0] {
            CropResult::Single(crop) => {
                assert!((crop.x - 100.0).abs() < 0.001);
                assert!((crop.y - 300.0).abs() < 0.001); // destination y
                assert!((crop.width - 400.0).abs() < 0.001); // destination width
                assert!((crop.height - 500.0).abs() < 0.001); // destination height
            }
            _ => panic!("Expected Single crop result"),
        }

        // Check last frame (should be destination)
        match &result[num_frames - 1] {
            CropResult::Single(crop) => {
                assert!((crop.x - 200.0).abs() < 0.001);
                assert!((crop.y - 300.0).abs() < 0.001);
                assert!((crop.width - 400.0).abs() < 0.001);
                assert!((crop.height - 500.0).abs() < 0.001);
            }
            _ => panic!("Expected Single crop result"),
        }

        // Check middle frame (should have halfway x, destination y/width/height)
        match &result[2] {
            CropResult::Single(crop) => {
                assert!((crop.x - 150.0).abs() < 0.001);
                assert!((crop.y - 300.0).abs() < 0.001); // destination y
                assert!((crop.width - 400.0).abs() < 0.001); // destination width
                assert!((crop.height - 500.0).abs() < 0.001); // destination height
            }
            _ => panic!("Expected Single crop result"),
        }
    }

    #[test]
    fn test_interpolate_crop_results_single_to_stacked() {
        use super::interpolate_crop_results;
        use crate::crop::{CropArea, CropResult};

        let start = CropResult::Single(CropArea::new(100.0, 200.0, 300.0, 400.0));
        let destination = CropResult::Stacked(
            CropArea::new(0.0, 0.0, 500.0, 600.0),
            CropArea::new(500.0, 0.0, 500.0, 600.0),
        );
        let num_frames = 3;

        let result = interpolate_crop_results(&start, &destination, num_frames);

        assert_eq!(result.len(), num_frames);

        // All results should be the destination (stacked)
        for crop_result in &result {
            match crop_result {
                CropResult::Stacked(crop1, crop2) => {
                    assert!((crop1.x - 0.0).abs() < 0.001);
                    assert!((crop1.y - 0.0).abs() < 0.001);
                    assert!((crop1.width - 500.0).abs() < 0.001);
                    assert!((crop1.height - 600.0).abs() < 0.001);
                    assert!((crop2.x - 500.0).abs() < 0.001);
                    assert!((crop2.y - 0.0).abs() < 0.001);
                    assert!((crop2.width - 500.0).abs() < 0.001);
                    assert!((crop2.height - 600.0).abs() < 0.001);
                }
                _ => panic!("Expected Stacked crop result"),
            }
        }
    }

    #[test]
    fn test_interpolate_crop_results_stacked_to_single() {
        use super::interpolate_crop_results;
        use crate::crop::{CropArea, CropResult};

        let start = CropResult::Stacked(
            CropArea::new(0.0, 0.0, 500.0, 600.0),
            CropArea::new(500.0, 0.0, 500.0, 600.0),
        );
        let destination = CropResult::Single(CropArea::new(200.0, 300.0, 400.0, 500.0));
        let num_frames = 3;

        let result = interpolate_crop_results(&start, &destination, num_frames);

        assert_eq!(result.len(), num_frames);

        // All results should be the destination (single)
        for crop_result in &result {
            match crop_result {
                CropResult::Single(crop) => {
                    assert!((crop.x - 200.0).abs() < 0.001);
                    assert!((crop.y - 300.0).abs() < 0.001);
                    assert!((crop.width - 400.0).abs() < 0.001);
                    assert!((crop.height - 500.0).abs() < 0.001);
                }
                _ => panic!("Expected Single crop result"),
            }
        }
    }

    #[test]
    fn test_interpolate_crop_results_stacked_to_stacked() {
        use super::interpolate_crop_results;
        use crate::crop::{CropArea, CropResult};

        let start = CropResult::Stacked(
            CropArea::new(0.0, 0.0, 500.0, 600.0),
            CropArea::new(500.0, 0.0, 500.0, 600.0),
        );
        let destination = CropResult::Stacked(
            CropArea::new(100.0, 100.0, 600.0, 700.0),
            CropArea::new(700.0, 100.0, 600.0, 700.0),
        );
        let num_frames = 3;

        let result = interpolate_crop_results(&start, &destination, num_frames);

        assert_eq!(result.len(), num_frames);

        // All results should be the destination (stacked)
        for crop_result in &result {
            match crop_result {
                CropResult::Stacked(crop1, crop2) => {
                    assert!((crop1.x - 100.0).abs() < 0.001);
                    assert!((crop1.y - 100.0).abs() < 0.001);
                    assert!((crop1.width - 600.0).abs() < 0.001);
                    assert!((crop1.height - 700.0).abs() < 0.001);
                    assert!((crop2.x - 700.0).abs() < 0.001);
                    assert!((crop2.y - 100.0).abs() < 0.001);
                    assert!((crop2.width - 600.0).abs() < 0.001);
                    assert!((crop2.height - 700.0).abs() < 0.001);
                }
                _ => panic!("Expected Stacked crop result"),
            }
        }
    }

    #[test]
    fn test_interpolate_crop_results_zero_frames() {
        use super::interpolate_crop_results;
        use crate::crop::{CropArea, CropResult};

        let start = CropResult::Single(CropArea::new(100.0, 200.0, 300.0, 400.0));
        let destination = CropResult::Single(CropArea::new(200.0, 300.0, 400.0, 500.0));

        let result = interpolate_crop_results(&start, &destination, 0);

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_interpolate_crop_results_one_frame() {
        use super::interpolate_crop_results;
        use crate::crop::{CropArea, CropResult};

        let start = CropResult::Single(CropArea::new(100.0, 200.0, 300.0, 400.0));
        let destination = CropResult::Single(CropArea::new(200.0, 300.0, 400.0, 500.0));

        let result = interpolate_crop_results(&start, &destination, 1);

        assert_eq!(result.len(), 1);
        match &result[0] {
            CropResult::Single(crop) => {
                assert!((crop.x - 200.0).abs() < 0.001);
                assert!((crop.y - 300.0).abs() < 0.001);
                assert!((crop.width - 400.0).abs() < 0.001);
                assert!((crop.height - 500.0).abs() < 0.001);
            }
            _ => panic!("Expected Single crop result"),
        }
    }

    #[test]
    fn test_interpolate_crop_results_identical_crops() {
        use super::interpolate_crop_results;
        use crate::crop::{CropArea, CropResult};

        let crop_area = CropArea::new(100.0, 200.0, 300.0, 400.0);
        let start = CropResult::Single(crop_area.clone());
        let destination = CropResult::Single(crop_area);
        let num_frames = 5;

        let result = interpolate_crop_results(&start, &destination, num_frames);

        assert_eq!(result.len(), num_frames);

        // All frames should be identical (since start and destination are the same)
        for crop_result in &result {
            match crop_result {
                CropResult::Single(crop) => {
                    assert!((crop.x - 100.0).abs() < 0.001);
                    assert!((crop.y - 200.0).abs() < 0.001);
                    assert!((crop.width - 300.0).abs() < 0.001);
                    assert!((crop.height - 400.0).abs() < 0.001);
                }
                _ => panic!("Expected Single crop result"),
            }
        }
    }

    #[test]
    fn test_interpolate_crop_results_large_frame_count() {
        use super::interpolate_crop_results;
        use crate::crop::{CropArea, CropResult};

        let start = CropResult::Single(CropArea::new(0.0, 0.0, 100.0, 100.0));
        let destination = CropResult::Single(CropArea::new(1000.0, 1000.0, 200.0, 200.0));
        let num_frames = 100;

        let result = interpolate_crop_results(&start, &destination, num_frames);

        assert_eq!(result.len(), num_frames);

        // Check that interpolation is smooth and monotonic
        for i in 1..num_frames {
            let prev = &result[i - 1];
            let curr = &result[i];

            if let (CropResult::Single(prev_crop), CropResult::Single(curr_crop)) = (prev, curr) {
                // Only X should be increasing (y, width, height stay constant)
                assert!(curr_crop.x >= prev_crop.x);
                // Y, width, and height should remain constant (destination values)
                assert!((curr_crop.y - 1000.0).abs() < 0.001); // destination y
                assert!((curr_crop.width - 200.0).abs() < 0.001); // destination width
                assert!((curr_crop.height - 200.0).abs() < 0.001); // destination height
            }
        }
    }
}
