use usls::{Hbb, Y};
use anyhow::Result;

/// Represents a crop area in the image
#[derive(Debug, Clone, PartialEq)]
pub struct CropArea {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl CropArea {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    /// Checks if this crop area is within the specified percentage of another crop area
    /// 
    /// # Arguments
    /// * `other` - The other crop area to compare against
    /// * `threshold_percent` - The maximum allowed difference as a percentage (e.g. 5.0 for 5%)
    /// 
    /// # Returns
    /// `true` if all dimensions (x, y, width, height) are within the threshold percentage of each other
    pub fn is_within_percentage(&self, other: &CropArea, threshold_percent: f32) -> bool {
        let threshold = threshold_percent / 100.0;
        
        // Helper function to check if two values are within threshold percentage of each other
        let is_within_threshold = |_label: &str, a: f32, b: f32| -> bool {
            if a == 0.0 && b == 0.0 {
                true
            } else if a == 0.0 || b == 0.0 {
                false
            } else {
                let diff = (a - b).abs();
                let max = a.max(b);
                let percent = diff / max;
                percent <= threshold + f32::EPSILON
            }
        };

        let x_ok = is_within_threshold("x", self.x, other.x);
        let y_ok = is_within_threshold("y", self.y, other.y);
        let w_ok = is_within_threshold("width", self.width, other.width);
        let h_ok = is_within_threshold("height", self.height, other.height);
        x_ok && y_ok && w_ok && h_ok
    }
}

/// Represents the result of calculating crop areas
#[derive(Debug, Clone)]
pub enum CropResult {
    /// A single crop area
    Single(CropArea),
    /// Two crop areas that should be stacked vertically
    Stacked(CropArea, CropArea),
}

/// Calculates crop area when no heads are detected
pub fn calculate_no_heads_crop(frame_width: f32, frame_height: f32) -> CropResult {
    // For no heads, we'll create a centered crop with 3:4 aspect ratio
    // The height will match the frame height
    let height = frame_height;
    let width = height * (3.0 / 4.0);
    
    // Center the crop horizontally
    let x = (frame_width - width) / 2.0;
    let y = 0.0;
    
    CropResult::Single(CropArea::new(x, y, width, height))
}

/// Calculates crop area for a single head
pub fn calculate_single_head_crop(frame_width: f32, frame_height: f32, head: &Hbb) -> CropResult {
    let head_center_x = head.cx();
    
    // Set height to match frame height
    let height = frame_height;
    // Set width to 3/4 of the height (3:4 aspect ratio)
    let width = height * (3.0 / 4.0);
    
    // Calculate initial x position centered on the head
    let mut x = head_center_x - width / 2.0;
    
    // Clamp x to frame bounds
    if x < 0.0 {
        x = 0.0;  // Clamp to left edge
    } else if x + width > frame_width {
        x = frame_width - width;  // Clamp to right edge
    }
    
    CropResult::Single(CropArea::new(x, 0.0, width, height))
}

/// Calculates crop area for two heads
pub fn calculate_two_heads_crop(frame_width: f32, frame_height: f32, head1: &Hbb, head2: &Hbb) -> CropResult {
    // Calculate the bounding box of the two heads
    let bbox = calculate_bounding_box(&[head1, head2]);

    // Check if the width of the bounding box is less than or equal to 3/4 of the frame height
    if bbox.width <= frame_height * 0.75 {
        // Return a single crop centered on the bounding box
        let crop_width = frame_height * 0.75;
        let crop_height = frame_height;
        let crop_x = bbox.x - (crop_width - bbox.width) / 2.0;
        let crop_y = 0.0; // Start at the top of the frame

        CropResult::Single(CropArea::new(crop_x, crop_y, crop_width, crop_height))
    } else {
        // Return two crops with specific dimensions and positions
        let crop_height = frame_height * (8.0/9.0);
        let crop_width = frame_height;
        
        // Calculate default y position (1/18 of frame height)
        let default_y = frame_height / 18.0;
        
        // Check if either head is at the top or bottom
        let head1_top = head1.ymin();
        let head1_bottom = head1.ymax();
        let head2_top = head2.ymin();
        let head2_bottom = head2.ymax();
        
        // If any head is above the default y position, set y to 0
        let crop1_y = if head1_top < default_y || head2_top < default_y {
            0.0
        } else {
            default_y
        };
        
        // If any head is below 17/18 of the height, set y to 1/9 of the height
        let crop2_y = if head1_bottom > frame_height * (17.0/18.0) || head2_bottom > frame_height * (17.0/18.0) {
            frame_height / 9.0
        } else {
            default_y
        };

        // Calculate default x positions
        let mut crop1_x = 0.0;
        let mut crop2_x = frame_width - frame_height;

        // Calculate how much of each head is in each crop
        let head1_in_crop1 = (head1.xmax().min(crop1_x + crop_width) - head1.xmin().max(crop1_x)).max(0.0);
        let head1_in_crop2 = (head1.xmax().min(crop2_x + crop_width) - head1.xmin().max(crop2_x)).max(0.0);
        let head2_in_crop1 = (head2.xmax().min(crop1_x + crop_width) - head2.xmin().max(crop1_x)).max(0.0);
        let head2_in_crop2 = (head2.xmax().min(crop2_x + crop_width) - head2.xmin().max(crop2_x)).max(0.0);

        // If head1 spans both crops
        if head1_in_crop1 > 0.0 && head1_in_crop2 > 0.0 {
            // Adjust the crop that contains more of head1
            if head1_in_crop1 >= head1_in_crop2 {
                // Move crop1 to fully include head1
                crop1_x = (head1.xmin() + head1.xmax() - crop_width) / 2.0;
                // Clamp to frame bounds
                crop1_x = crop1_x.max(0.0).min(frame_width - crop_width);
            } else {
                // Move crop2 to fully include head1
                crop2_x = (head1.xmin() + head1.xmax() - crop_width) / 2.0;
                // Clamp to frame bounds
                crop2_x = crop2_x.max(0.0).min(frame_width - crop_width);
            }
        }

        // If head2 spans both crops
        if head2_in_crop1 > 0.0 && head2_in_crop2 > 0.0 {
            // Adjust the crop that contains more of head2
            if head2_in_crop2 >= head2_in_crop1 {
                // Move crop2 to fully include head2
                crop2_x = (head2.xmin() + head2.xmax() - crop_width) / 2.0;
                // Clamp to frame bounds
                crop2_x = crop2_x.max(0.0).min(frame_width - crop_width);
            } else {
                // Move crop1 to fully include head2
                crop1_x = (head2.xmin() + head2.xmax() - crop_width) / 2.0;
                // Clamp to frame bounds
                crop1_x = crop1_x.max(0.0).min(frame_width - crop_width);
            }
        }
        
        // First crop
        let crop1 = CropArea::new(crop1_x, crop1_y, crop_width, crop_height);
        
        // Second crop
        let crop2 = CropArea::new(crop2_x, crop2_y, crop_width, crop_height);

        CropResult::Stacked(crop1, crop2)
    }
}

/// Calculates crop area for three to five heads
pub fn calculate_three_to_five_heads_crop(frame_width: f32, frame_height: f32, heads: &[&Hbb]) -> CropResult {
    // Calculate the bounding box that contains all heads
    let bbox = calculate_bounding_box(heads);
    
    // If the bounding box width is less than or equal to 3/4 of the frame height,
    // we can fit all heads in a single crop
    if bbox.width <= frame_height * (3.0/4.0) {
        // Set height to match frame height
        let height = frame_height;
        // Set width to 3/4 of the height (3:4 aspect ratio)
        let width = height * (3.0 / 4.0);
        
        // Calculate x position centered on the bounding box
        let mut x = bbox.x + bbox.width/2.0 - width/2.0;
        
        // Clamp x to frame bounds
        if x < 0.0 {
            x = 0.0;  // Clamp to left edge
        } else if x + width > frame_width {
            x = frame_width - width;  // Clamp to right edge
        }
        
        CropResult::Single(CropArea::new(x, 0.0, width, height))
    } else {
        // For far heads, use assignment-based approach
        let crop_height = frame_height * (8.0/9.0);
        let crop_width = frame_height;
        let crop_y = frame_height / 18.0;
        
        // Default crop positions
        let mut x1 = 0.0;
        let mut x2 = frame_width - crop_width;
        let crop1_default = CropArea::new(x1, crop_y, crop_width, crop_height);
        let crop2_default = CropArea::new(x2, crop_y, crop_width, crop_height);
        
        // Check if all heads are fully contained in at least one default crop
        let all_heads_contained = heads.iter().all(|head| {
            let head_xmin = head.xmin();
            let head_xmax = head.xmax();
            let in_crop1 = head_xmin >= crop1_default.x && head_xmax <= crop1_default.x + crop1_default.width;
            let in_crop2 = head_xmin >= crop2_default.x && head_xmax <= crop2_default.x + crop2_default.width;
            in_crop1 || in_crop2
        });
        
        if all_heads_contained {
            // Use default crops to maximize frame coverage
            return CropResult::Stacked(crop1_default, crop2_default);
        }
        
        // Otherwise, assign heads to crops based on which side of the frame they're closer to
        let mut crop1_heads = Vec::new();
        let mut crop2_heads = Vec::new();
        let frame_center = frame_width / 2.0;
        for head in heads {
            let head_center = head.cx();
            if head_center < frame_center {
                crop1_heads.push(*head);
            } else {
                crop2_heads.push(*head);
            }
        }
        // Position crop1 to contain all its assigned heads
        if !crop1_heads.is_empty() {
            let min_x = crop1_heads.iter().map(|h| h.xmin()).fold(f32::MAX, f32::min);
            let max_x = crop1_heads.iter().map(|h| h.xmax()).fold(f32::MIN, f32::max);
            if max_x - min_x > crop_width {
                x1 = (min_x + max_x - crop_width) / 2.0;
            } else {
                x1 = min_x;
            }
            x1 = x1.max(0.0).min(frame_width - crop_width);
        }
        // Position crop2 to contain all its assigned heads
        if !crop2_heads.is_empty() {
            let min_x = crop2_heads.iter().map(|h| h.xmin()).fold(f32::MAX, f32::min);
            let max_x = crop2_heads.iter().map(|h| h.xmax()).fold(f32::MIN, f32::max);
            if max_x - min_x > crop_width {
                x2 = (min_x + max_x - crop_width) / 2.0;
            } else {
                x2 = max_x - crop_width;
            }
            x2 = x2.max(0.0).min(frame_width - crop_width);
        }
        // Create the crops
        let mut crop1 = CropArea::new(x1, crop_y, crop_width, crop_height);
        let mut crop2 = CropArea::new(x2, crop_y, crop_width, crop_height);
        // Verify that every head is fully contained in at least one crop, adjust if not
        for head in heads {
            let head_xmin = head.xmin();
            let head_xmax = head.xmax();
            let head_center = head.cx();
            let in_crop1 = head_xmin >= crop1.x && head_xmax <= crop1.x + crop1.width;
            let in_crop2 = head_xmin >= crop2.x && head_xmax <= crop2.x + crop2.width;
            if !in_crop1 && !in_crop2 {
                let dist_to_crop1 = (head_center - (crop1.x + crop1.width/2.0)).abs();
                let dist_to_crop2 = (head_center - (crop2.x + crop2.width/2.0)).abs();
                if dist_to_crop1 <= dist_to_crop2 {
                    let new_x1 = head_xmin;
                    x1 = new_x1.max(0.0).min(frame_width - crop_width);
                    crop1 = CropArea::new(x1, crop_y, crop_width, crop_height);
                } else {
                    let new_x2 = head_xmax - crop_width;
                    x2 = new_x2.max(0.0).min(frame_width - crop_width);
                    crop2 = CropArea::new(x2, crop_y, crop_width, crop_height);
                }
            }
        }
        CropResult::Stacked(crop1, crop2)
    }
}

/// Calculates crop area for more than five heads
pub fn calculate_more_than_five_heads_crop(frame_width: f32, frame_height: f32, heads: &[&Hbb]) -> CropResult {
    // Find the largest head by area
    let largest_head = heads
        .iter()
        .max_by(|a, b| a.area().partial_cmp(&b.area()).unwrap())
        .unwrap();
    let head_center_x = largest_head.cx();
    
    // Set height to match frame height
    let height = frame_height;
    // Set width to 3/4 of the height (3:4 aspect ratio)
    let width = height * (3.0 / 4.0);
    
    // Calculate initial x position centered on the largest head
    let mut x = head_center_x - width / 2.0;
    
    // Clamp x to frame bounds
    if x < 0.0 {
        x = 0.0;  // Clamp to left edge
    } else if x + width > frame_width {
        x = frame_width - width;  // Clamp to right edge
    }
    
    CropResult::Single(CropArea::new(x, 0.0, width, height))
}

/// Calculates the optimal crop area based on detected heads
/// 
/// # Arguments
/// * `frame_width` - Width of the input frame
/// * `frame_height` - Height of the input frame
/// * `detection` - The YOLO detection results for the frame
/// * `head_prob_threshold` - Minimum probability threshold for considering a detection as a head
pub fn calculate_crop_area(
    frame_width: f32,
    frame_height: f32,
    detection: &Y,
    head_prob_threshold: f32,
) -> Result<CropResult> {
    // Get all head detections above the probability threshold
    let heads: Vec<&Hbb> = if let Some(hbbs) = detection.hbbs() {
        hbbs.iter()
            .filter(|hbb| {
                if let Some(confidence) = hbb.confidence() {
                    confidence >= head_prob_threshold
                } else {
                    false
                }
            })
            .collect()
    } else {
        vec![]
    };

    match heads.len() {
        0 => Ok(calculate_no_heads_crop(frame_width, frame_height)),
        1 => Ok(calculate_single_head_crop(frame_width, frame_height, heads[0])),
        2 => Ok(calculate_two_heads_crop(frame_width, frame_height, heads[0], heads[1])),
        3..=5 => Ok(calculate_three_to_five_heads_crop(frame_width, frame_height, &heads)),
        _ => Ok(calculate_more_than_five_heads_crop(frame_width, frame_height, &heads)),
    }
}

/// Calculates the bounding box that contains all given heads
pub fn calculate_bounding_box(heads: &[&Hbb]) -> CropArea {
    if heads.is_empty() {
        return CropArea::new(0.0, 0.0, 0.0, 0.0);
    }

    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;

    for head in heads {
        let xmin = head.cx() - head.width() / 2.0;
        let ymin = head.cy() - head.height() / 2.0;
        let xmax = head.cx() + head.width() / 2.0;
        let ymax = head.cy() + head.height() / 2.0;

        min_x = min_x.min(xmin);
        min_y = min_y.min(ymin);
        max_x = max_x.max(xmax);
        max_y = max_y.max(ymax);
    }

    CropArea::new(
        min_x,
        min_y,
        max_x - min_x,
        max_y - min_y,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use usls::Y;

    #[test]
    fn test_calculate_bounding_box() {

        // Test single head
        let head = Hbb::from_xywh(300.0, 300.0, 100.0, 100.0);
        let bbox = calculate_bounding_box(&[&head]);
        assert!((bbox.x - 300.0).abs() < 1.0);
        assert!((bbox.y - 300.0).abs() < 1.0);
        assert!((bbox.width - 100.0).abs() < 1.0);
        assert!((bbox.height - 100.0).abs() < 1.0);

        // Test two heads
        let head1 = Hbb::from_xywh(300.0, 300.0, 100.0, 100.0);
        let head2 = Hbb::from_xywh(1000.0, 300.0, 100.0, 100.0);
        let bbox = calculate_bounding_box(&[&head1, &head2]);
        assert!((bbox.x - 300.0).abs() < 1.0);
        assert!((bbox.y - 300.0).abs() < 1.0);
        assert!((bbox.width - 800.0).abs() < 1.0);
        assert!((bbox.height - 100.0).abs() < 1.0);

        // Test three heads in a triangle formation
        let head1 = Hbb::from_xywh(300.0, 300.0, 100.0, 100.0);
        let head2 = Hbb::from_xywh(1000.0, 300.0, 100.0, 100.0);
        let head3 = Hbb::from_xywh(1000.0, 1000.0, 100.0, 100.0);
        let bbox = calculate_bounding_box(&[&head1, &head2, &head3]);
        assert!((bbox.x - 300.0).abs() < 1.0);
        assert!((bbox.y - 300.0).abs() < 1.0);
        assert!((bbox.width - 800.0).abs() < 1.0);
        assert!((bbox.height - 800.0).abs() < 1.0);

        // Test empty vector
        let bbox = calculate_bounding_box(&[]);
        assert_eq!(bbox.x, 0.0);
        assert_eq!(bbox.y, 0.0);
        assert_eq!(bbox.width, 0.0);
        assert_eq!(bbox.height, 0.0);
    }

    #[test]
    fn test_calculate_no_heads_crop() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;
        
        let crop = calculate_no_heads_crop(frame_width, frame_height);
        
        match crop {
            CropResult::Single(crop) => {
                // Height should match frame height
                assert!((crop.height - frame_height).abs() < 1.0);
                
                // Width should be 3/4 of the height (3:4 aspect ratio)
                let expected_width = frame_height * (3.0/4.0);
                assert!((crop.width - expected_width).abs() < 1.0);
                
                // Should be centered horizontally
                let expected_x = (frame_width - expected_width) / 2.0;
                assert!((crop.x - expected_x).abs() < 1.0);
                
                // Should start at y = 0
                assert!(crop.y.abs() < 1.0);
                
                // Should be within frame bounds
                assert!(crop.x >= 0.0);
                assert!(crop.y >= 0.0);
                assert!(crop.x + crop.width <= frame_width);
                assert!(crop.y + crop.height <= frame_height);
            }
            _ => panic!("Expected single crop for no heads case"),
        }
    }

    #[test]
    fn test_calculate_single_head_crop() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;
        
        // Test centered head
        let head = Hbb::from_cxcywh(frame_width/2.0, frame_height/2.0, 100.0, 100.0);
        let crop = calculate_single_head_crop(frame_width, frame_height, &head);
        
        match crop {
            CropResult::Single(crop) => {
                // Height should match frame height
                assert!((crop.height - frame_height).abs() < 1.0);
                
                // Width should be 3/4 of the height (3:4 aspect ratio)
                let expected_width = frame_height * (3.0/4.0);
                assert!((crop.width - expected_width).abs() < 1.0);
                
                // Should be centered on the head's x-coordinate
                assert!((crop.x + crop.width/2.0 - frame_width/2.0).abs() < 1.0);
                
                // Should be within frame bounds
                assert!(crop.x >= 0.0);
                assert!(crop.y >= 0.0);
                assert!(crop.x + crop.width <= frame_width);
                assert!(crop.y + crop.height <= frame_height);
            }
            _ => panic!("Expected single crop for single head case"),
        }

        // Test head on far left
        let head = Hbb::from_cxcywh(50.0, frame_height/2.0, 100.0, 100.0);
        let crop = calculate_single_head_crop(frame_width, frame_height, &head);
        
        match crop {
            CropResult::Single(crop) => {
                // Height should match frame height
                assert!((crop.height - frame_height).abs() < 1.0);
                
                // Width should be 3/4 of the height (3:4 aspect ratio)
                let expected_width = frame_height * (3.0/4.0);
                assert!((crop.width - expected_width).abs() < 1.0);
                
                // Should be clamped to left edge
                assert!(crop.x.abs() < 1.0);
                
                // Should be within frame bounds
                assert!(crop.x >= 0.0);
                assert!(crop.y >= 0.0);
                assert!(crop.x + crop.width <= frame_width);
                assert!(crop.y + crop.height <= frame_height);
            }
            _ => panic!("Expected single crop for single head case"),
        }

        // Test head on far right
        let head = Hbb::from_cxcywh(frame_width - 50.0, frame_height/2.0, 100.0, 100.0);
        let crop = calculate_single_head_crop(frame_width, frame_height, &head);
        
        match crop {
            CropResult::Single(crop) => {
                // Height should match frame height
                assert!((crop.height - frame_height).abs() < 1.0);
                
                // Width should be 3/4 of the height (3:4 aspect ratio)
                let expected_width = frame_height * (3.0/4.0);
                assert!((crop.width - expected_width).abs() < 1.0);
                
                // Should be clamped to right edge
                assert!((crop.x + crop.width - frame_width).abs() < 1.0);
                
                // Should be within frame bounds
                assert!(crop.x >= 0.0);
                assert!(crop.y >= 0.0);
                assert!(crop.x + crop.width <= frame_width);
                assert!(crop.y + crop.height <= frame_height);
            }
            _ => panic!("Expected single crop for single head case"),
        }
    }

    #[test]
    fn test_calculate_two_heads_crop_close() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;
        
        // Test close heads - heads are within 3/4 of frame height
        let head1 = Hbb::from_xywh(300.0, 300.0, 100.0, 100.0);
        let head2 = Hbb::from_xywh(450.0, 300.0, 100.0, 100.0);
        let crop = calculate_two_heads_crop(frame_width, frame_height, &head1, &head2);
        
        match crop {
            CropResult::Single(crop) => {
                // Height should match frame height
                assert!((crop.height - frame_height).abs() < 1.0);
                
                // Width should be 3/4 of the height
                let expected_width = frame_height * (3.0/4.0);
                assert!((crop.width - expected_width).abs() < 1.0);
                
                // Calculate the center of the bounding box
                let bbox_center_x = 425.0;
                
                // Should be centered on the bounding box center
                assert!((crop.x + crop.width/2.0 - bbox_center_x).abs() < 1.0);
                
                // Should be within frame bounds
                assert!(crop.x >= 0.0);
                assert!(crop.y >= 0.0);
                assert!(crop.x + crop.width <= frame_width);
                assert!(crop.y + crop.height <= frame_height);
                
                // Should contain both heads
                assert!(crop.x <= head1.x());
                assert!(crop.x + crop.width >= head2.x() + head2.width());
            }
            _ => panic!("Expected single crop for close heads case"),
        }
    }

    #[test]
    fn test_calculate_two_heads_crop_far() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;
        
        // Test far heads - heads are more than 3/4 of frame height apart
        let head1 = Hbb::from_cxcywh(frame_width/4.0, frame_height/2.0, 100.0, 100.0);
        let head2 = Hbb::from_cxcywh(3.0 * frame_width/4.0, frame_height/2.0, 100.0, 100.0);
        let crop = calculate_two_heads_crop(frame_width, frame_height, &head1, &head2);
        
        match crop {
            CropResult::Stacked(crop1, crop2) => {
                // Both crops should have height of 8/9 of frame height
                let expected_height = frame_height * (8.0/9.0);
                assert!((crop1.height - expected_height).abs() < 1.0);
                assert!((crop2.height - expected_height).abs() < 1.0);
                
                // Both crops should have width equal to frame height
                assert!((crop1.width - frame_height).abs() < 1.0);
                assert!((crop2.width - frame_height).abs() < 1.0);
                
                // First crop should be at x=0
                assert!(crop1.x.abs() < 1.0);
                
                // Second crop should be at x = frame_width - frame_height
                assert!((crop2.x - (frame_width - frame_height)).abs() < 1.0);
                
                // Both crops should be at y = frame_height/18
                let expected_y = frame_height / 18.0;
                assert!((crop1.y - expected_y).abs() < 1.0);
                assert!((crop2.y - expected_y).abs() < 1.0);
                
                // Both crops should be within frame bounds
                assert!(crop1.x >= 0.0);
                assert!(crop1.y >= 0.0);
                assert!(crop1.x + crop1.width <= frame_width);
                assert!(crop1.y + crop1.height <= frame_height);
                assert!(crop2.x >= 0.0);
                assert!(crop2.y >= 0.0);
                assert!(crop2.x + crop2.width <= frame_width);
                assert!(crop2.y + crop2.height <= frame_height);
            }
            _ => panic!("Expected stacked crops for far heads case"),
        }
    }

    #[test]
    fn test_calculate_two_heads_crop_far_with_edge_heads() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;
        
        // Test with one head at the top and one at the bottom
        let head1 = Hbb::from_cxcywh(frame_width/4.0, 50.0, 100.0, 100.0); // Head near top
        let head2 = Hbb::from_cxcywh(3.0 * frame_width/4.0, frame_height - 50.0, 100.0, 100.0); // Head near bottom
        let crop = calculate_two_heads_crop(frame_width, frame_height, &head1, &head2);
        
        match crop {
            CropResult::Stacked(crop1, crop2) => {
                // Both crops should have height of 8/9 of frame height
                let expected_height = frame_height * (8.0/9.0);
                assert!((crop1.height - expected_height).abs() < 1.0);
                assert!((crop2.height - expected_height).abs() < 1.0);
                
                // Both crops should have width equal to frame height
                assert!((crop1.width - frame_height).abs() < 1.0);
                assert!((crop2.width - frame_height).abs() < 1.0);
                
                // First crop should be at x=0
                assert!(crop1.x.abs() < 1.0);
                
                // Second crop should be at x = frame_width - frame_height
                assert!((crop2.x - (frame_width - frame_height)).abs() < 1.0);
                
                // First crop should be at y=0 since head1 is near the top
                assert!(crop1.y.abs() < 1.0);
                
                // Second crop should be at y = frame_height/9 since head2 is near the bottom
                assert!((crop2.y - frame_height/9.0).abs() < 1.0);
                
                // Both crops should be within frame bounds
                assert!(crop1.x >= 0.0);
                assert!(crop1.y >= 0.0);
                assert!(crop1.x + crop1.width <= frame_width);
                assert!(crop1.y + crop1.height <= frame_height);
                assert!(crop2.x >= 0.0);
                assert!(crop2.y >= 0.0);
                assert!(crop2.x + crop2.width <= frame_width);
                assert!(crop2.y + crop2.height <= frame_height);
            }
            _ => panic!("Expected stacked crops for far heads case"),
        }
    }

    #[test]
    fn test_calculate_two_heads_crop_far_with_spanning_head() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;
        
        // Create a head that spans across both crops
        // The head's center is at frame_width/2, and it's wide enough to overlap both crops
        let head1 = Hbb::from_cxcywh(frame_width/2.0, frame_height/2.0, 400.0, 100.0);
        // Second head is far to the right, ensuring the bounding box is wider than 3/4 of frame height
        let head2 = Hbb::from_cxcywh(frame_width - 200.0, frame_height/2.0, 100.0, 100.0);
        
        let crop = calculate_two_heads_crop(frame_width, frame_height, &head1, &head2);
        
        match crop {
            CropResult::Stacked(crop1, crop2) => {
                // Both crops should have height of 8/9 of frame height
                let expected_height = frame_height * (8.0/9.0);
                assert!((crop1.height - expected_height).abs() < 1.0);
                assert!((crop2.height - expected_height).abs() < 1.0);
                
                // Both crops should have width equal to frame height
                assert!((crop1.width - frame_height).abs() < 1.0);
                assert!((crop2.width - frame_height).abs() < 1.0);
                
                // Both crops should be at y = frame_height/18
                let expected_y = frame_height / 18.0;
                assert!((crop1.y - expected_y).abs() < 1.0);
                assert!((crop2.y - expected_y).abs() < 1.0);
                
                // The head1 spans both crops, but more of it is in crop1
                // Verify that crop1 is adjusted to fully include head1
                assert!(crop1.x <= head1.xmin());
                assert!(crop1.x + crop1.width >= head1.xmax());
                
                // Verify that crop2 still contains head2
                assert!(crop2.x <= head2.xmin());
                assert!(crop2.x + crop2.width >= head2.xmax());
                
                // Both crops should be within frame bounds
                assert!(crop1.x >= 0.0);
                assert!(crop1.y >= 0.0);
                assert!(crop1.x + crop1.width <= frame_width);
                assert!(crop1.y + crop1.height <= frame_height);
                assert!(crop2.x >= 0.0);
                assert!(crop2.y >= 0.0);
                assert!(crop2.x + crop2.width <= frame_width);
                assert!(crop2.y + crop2.height <= frame_height);
            }
            _ => panic!("Expected stacked crops for far heads case"),
        }
    }

    #[test]
    fn test_calculate_three_to_five_heads_crop_close() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;
        
        // Create three heads that are close together (within 3/4 of frame height)
        let head1 = Hbb::from_cxcywh(frame_width/2.0 - 100.0, frame_height/2.0, 100.0, 100.0);
        let head2 = Hbb::from_cxcywh(frame_width/2.0, frame_height/2.0, 100.0, 100.0);
        let head3 = Hbb::from_cxcywh(frame_width/2.0 + 100.0, frame_height/2.0, 100.0, 100.0);
        let heads = vec![&head1, &head2, &head3];
        
        let crop = calculate_three_to_five_heads_crop(frame_width, frame_height, &heads);
        
        match crop {
            CropResult::Single(crop) => {
                // Height should match frame height
                assert!((crop.height - frame_height).abs() < 1.0);
                
                // Width should be 3/4 of the height
                let expected_width = frame_height * (3.0/4.0);
                assert!((crop.width - expected_width).abs() < 1.0);
                
                // Calculate the bounding box of all heads
                let bbox = calculate_bounding_box(&heads);
                let bbox_center_x = bbox.x + bbox.width/2.0;
                
                // Crop should be centered on the bounding box center
                assert!((crop.x + crop.width/2.0 - bbox_center_x).abs() < 1.0);
                
                // Should be within frame bounds
                assert!(crop.x >= 0.0);
                assert!(crop.y >= 0.0);
                assert!(crop.x + crop.width <= frame_width);
                assert!(crop.y + crop.height <= frame_height);
                
                // Should contain all heads
                assert!(crop.x <= head1.cx());
                assert!(crop.x + crop.width >= head3.cx());
            }
            _ => panic!("Expected single crop for close heads case"),
        }
    }

    #[test]
    fn test_calculate_three_to_five_heads_crop_far_default() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;
        
        // Create three heads that are far apart horizontally, but default crop positions are sufficient
        let head1 = Hbb::from_cxcywh(200.0, frame_height/2.0, 100.0, 100.0);
        let head2 = Hbb::from_cxcywh(1200.0, frame_height/2.0, 100.0, 100.0);
        let head3 = Hbb::from_cxcywh(1800.0, frame_height/2.0, 100.0, 100.0);
        let heads = vec![&head1, &head2, &head3];
        
        let crop = calculate_three_to_five_heads_crop(frame_width, frame_height, &heads);
        
        match crop {
            CropResult::Stacked(crop1, crop2) => {
                // Both crops should have height of 8/9 of frame height
                let expected_height = frame_height * (8.0/9.0);
                assert!((crop1.height - expected_height).abs() < 1.0);
                assert!((crop2.height - expected_height).abs() < 1.0);
                
                // Both crops should have width equal to frame height
                assert!((crop1.width - frame_height).abs() < 1.0);
                assert!((crop2.width - frame_height).abs() < 1.0);
                
                // Both crops should be at y = frame_height/18
                let expected_y = frame_height / 18.0;
                assert!((crop1.y - expected_y).abs() < 1.0);
                assert!((crop2.y - expected_y).abs() < 1.0);
                
                // First crop should be at x=0
                assert!(crop1.x.abs() < 1.0);
                
                // Second crop should be at x = frame_width - frame_height
                assert!((crop2.x - (frame_width - frame_height)).abs() < 1.0);
                
                // Both crops should be within frame bounds
                assert!(crop1.x >= 0.0);
                assert!(crop1.y >= 0.0);
                assert!(crop1.x + crop1.width <= frame_width);
                assert!(crop1.y + crop1.height <= frame_height);
                assert!(crop2.x >= 0.0);
                assert!(crop2.y >= 0.0);
                assert!(crop2.x + crop2.width <= frame_width);
                assert!(crop2.y + crop2.height <= frame_height);
                
                // Verify that each head is fully contained in at least one crop
                for head in &heads {
                    let head_xmin = head.xmin();
                    let head_xmax = head.xmax();
                    
                    // Check if head is in crop1
                    let in_crop1 = head_xmin >= crop1.x && head_xmax <= crop1.x + crop1.width;
                    // Check if head is in crop2
                    let in_crop2 = head_xmin >= crop2.x && head_xmax <= crop2.x + crop2.width;
                    
                    // Head should be fully contained in at least one crop
                    assert!(in_crop1 || in_crop2, "Head should be fully contained in at least one crop");
                }
            }
            _ => panic!("Expected stacked crops for far heads case"),
        }
    }

    #[test]
    fn test_calculate_three_to_five_heads_crop_far_adjusted() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;
        
        // Create three heads that are far apart horizontally, requiring crop positions to be adjusted
        let head1 = Hbb::from_cxcywh(400.0, frame_height/2.0, 100.0, 100.0);
        let head2 = Hbb::from_cxcywh(1000.0, frame_height/2.0, 200.0, 100.0);
        let head3 = Hbb::from_cxcywh(1600.0, frame_height/2.0, 100.0, 100.0);
        let heads = vec![&head1, &head2, &head3];
        
        let crop = calculate_three_to_five_heads_crop(frame_width, frame_height, &heads);
        
        match crop {
            CropResult::Stacked(crop1, crop2) => {
                // Both crops should have height of 8/9 of frame height
                let expected_height = frame_height * (8.0/9.0);
                assert!((crop1.height - expected_height).abs() < 1.0);
                assert!((crop2.height - expected_height).abs() < 1.0);
                
                // Both crops should have width equal to frame height
                assert!((crop1.width - frame_height).abs() < 1.0);
                assert!((crop2.width - frame_height).abs() < 1.0);
                
                // Both crops should be at y = frame_height/18
                let expected_y = frame_height / 18.0;
                assert!((crop1.y - expected_y).abs() < 1.0);
                assert!((crop2.y - expected_y).abs() < 1.0);
                
                // First crop should be adjusted to contain head1
                assert!(crop1.x <= head1.xmin());
                assert!(crop1.x + crop1.width >= head1.xmax());
                
                // Second crop should be adjusted to contain head3
                assert!(crop2.x <= head3.xmin());
                assert!(crop2.x + crop2.width >= head3.xmax());
                
                // Both crops should be within frame bounds
                assert!(crop1.x >= 0.0);
                assert!(crop1.y >= 0.0);
                assert!(crop1.x + crop1.width <= frame_width);
                assert!(crop1.y + crop1.height <= frame_height);
                assert!(crop2.x >= 0.0);
                assert!(crop2.y >= 0.0);
                assert!(crop2.x + crop2.width <= frame_width);
                assert!(crop2.y + crop2.height <= frame_height);
                
                // Verify that each head is fully contained in at least one crop
                for head in &heads {
                    let head_xmin = head.xmin();
                    let head_xmax = head.xmax();
                    
                    // Check if head is in crop1
                    let in_crop1 = head_xmin >= crop1.x && head_xmax <= crop1.x + crop1.width;
                    // Check if head is in crop2
                    let in_crop2 = head_xmin >= crop2.x && head_xmax <= crop2.x + crop2.width;
                    
                    // Head should be fully contained in at least one crop
                    assert!(in_crop1 || in_crop2, "Head should be fully contained in at least one crop");
                }
            }
            _ => panic!("Expected stacked crops for far heads case"),
        }
    }

    #[test]
    fn test_calculate_more_than_five_heads_crop() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;
        
        // Create 6 heads
        let head1 = Hbb::from_cxcywh(frame_width/6.0, frame_height/6.0, 100.0, 100.0);
        let head2 = Hbb::from_cxcywh(frame_width/3.0, frame_height/3.0, 100.0, 100.0);
        let head3 = Hbb::from_cxcywh(frame_width/2.0, frame_height/2.0, 100.0, 100.0);
        let head4 = Hbb::from_cxcywh(2.0 * frame_width/3.0, 2.0 * frame_height/3.0, 100.0, 100.0);
        let head5 = Hbb::from_cxcywh(5.0 * frame_width/6.0, 5.0 * frame_height/6.0, 100.0, 100.0);
        let head6 = Hbb::from_cxcywh(frame_width - 100.0, frame_height - 100.0, 100.0, 100.0);
        let heads = vec![&head1, &head2, &head3, &head4, &head5, &head6];
        
        let crop = calculate_more_than_five_heads_crop(frame_width, frame_height, &heads);
        
        match crop {
            CropResult::Single(crop) => {
                // Find the largest head by area
                let largest_head = heads.iter().max_by(|a, b| a.area().partial_cmp(&b.area()).unwrap()).unwrap();
                let head_center_x = largest_head.cx();
                
                // Height should match frame height
                assert!((crop.height - frame_height).abs() < 1.0);
                
                // Width should be 3/4 of the height (3:4 aspect ratio)
                let expected_width = frame_height * (3.0/4.0);
                assert!((crop.width - expected_width).abs() < 1.0);
                
                // Should be centered on the largest head unless at the edge
                let eps = 1e-3;
                if crop.x.abs() > eps && (frame_width - (crop.x + crop.width)).abs() > eps {
                    assert!((crop.x + crop.width/2.0 - head_center_x).abs() < 1.0);
                }
                
                // Should start at y = 0
                assert!(crop.y.abs() < 1.0);
                
                // Should be within frame bounds
                assert!(crop.x >= 0.0);
                assert!(crop.y >= 0.0);
                assert!(crop.x + crop.width <= frame_width);
                assert!(crop.y + crop.height <= frame_height);
            }
            _ => panic!("Expected single crop for more than 5 heads case"),
        }
    }

    #[test]
    fn test_calculate_more_than_five_heads_crop_edge_head() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;
        
        // Create heads with the largest one near the right edge
        let head1 = Hbb::from_cxcywh(frame_width/6.0, frame_height/6.0, 100.0, 100.0);
        let head2 = Hbb::from_cxcywh(frame_width/3.0, frame_height/3.0, 100.0, 100.0);
        let head3 = Hbb::from_cxcywh(frame_width/2.0, frame_height/2.0, 100.0, 100.0);
        let head4 = Hbb::from_cxcywh(2.0 * frame_width/3.0, 2.0 * frame_height/3.0, 100.0, 100.0);
        let head5 = Hbb::from_cxcywh(5.0 * frame_width/6.0, 5.0 * frame_height/6.0, 100.0, 100.0);
        // Make head6 the largest and position it near the right edge
        let head6 = Hbb::from_cxcywh(frame_width - 250.0, frame_height/2.0, 200.0, 200.0);
        let heads = vec![&head1, &head2, &head3, &head4, &head5, &head6];
        
        let crop = calculate_more_than_five_heads_crop(frame_width, frame_height, &heads);
        
        match crop {
            CropResult::Single(crop) => {
                // Height should match frame height
                assert!((crop.height - frame_height).abs() < 1.0);
                
                // Width should be 3/4 of the height (3:4 aspect ratio)
                let expected_width = frame_height * (3.0/4.0);
                assert!((crop.width - expected_width).abs() < 1.0);
                
                // Should be clamped to right edge since largest head is near the edge
                assert!((crop.x + crop.width - frame_width).abs() < 1.0);
                
                // Should start at y = 0
                assert!(crop.y.abs() < 1.0);
                
                // Should be within frame bounds
                assert!(crop.x >= 0.0);
                assert!(crop.y >= 0.0);
                assert!(crop.x + crop.width <= frame_width);
                assert!(crop.y + crop.height <= frame_height);
            }
            _ => panic!("Expected single crop for more than 5 heads case"),
        }
    }

    #[test]
    fn test_calculate_crop_area() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;
        let head_prob_threshold = 0.5;
        
        // Test no heads
        let detection = Y::default();
        let crop = calculate_crop_area(frame_width, frame_height, &detection, head_prob_threshold).unwrap();
        assert!(matches!(crop, CropResult::Single(_)));
        
        // Test single head
        let mut detection = Y::default();
        let head = Hbb::from_cxcywh(frame_width/2.0, frame_height/2.0, 100.0, 100.0)
            .with_confidence(0.9);
        let hbbs = vec![head];
        detection = detection.with_hbbs(&hbbs);
        let crop = calculate_crop_area(frame_width, frame_height, &detection, head_prob_threshold).unwrap();
        assert!(matches!(crop, CropResult::Single(_)));
        
        // Test two heads
        let mut detection = Y::default();
        let head1 = Hbb::from_cxcywh(frame_width/4.0, frame_height/2.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head2 = Hbb::from_cxcywh(3.0 * frame_width/4.0, frame_height/2.0, 100.0, 100.0)
            .with_confidence(0.9);
        let hbbs = vec![head1, head2];
        detection = detection.with_hbbs(&hbbs);
        let crop = calculate_crop_area(frame_width, frame_height, &detection, head_prob_threshold).unwrap();
        assert!(matches!(crop, CropResult::Stacked(_, _)));
        
        // Test three heads
        let mut detection = Y::default();
        let head1 = Hbb::from_cxcywh(frame_width/4.0, frame_height/4.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head2 = Hbb::from_cxcywh(frame_width/2.0, frame_height/2.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head3 = Hbb::from_cxcywh(3.0 * frame_width/4.0, 3.0 * frame_height/4.0, 100.0, 100.0)
            .with_confidence(0.9);
        let hbbs = vec![head1, head2, head3];
        detection = detection.with_hbbs(&hbbs);
        let crop = calculate_crop_area(frame_width, frame_height, &detection, head_prob_threshold).unwrap();
        assert!(matches!(crop, CropResult::Stacked(_, _)));
        
        // Test more than five heads
        let mut detection = Y::default();
        let head1 = Hbb::from_cxcywh(frame_width/6.0, frame_height/6.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head2 = Hbb::from_cxcywh(frame_width/3.0, frame_height/3.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head3 = Hbb::from_cxcywh(frame_width/2.0, frame_height/2.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head4 = Hbb::from_cxcywh(2.0 * frame_width/3.0, 2.0 * frame_height/3.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head5 = Hbb::from_cxcywh(5.0 * frame_width/6.0, 5.0 * frame_height/6.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head6 = Hbb::from_cxcywh(frame_width - 100.0, frame_height - 100.0, 100.0, 100.0)
            .with_confidence(0.9);
        let hbbs = vec![head1, head2, head3, head4, head5, head6];
        detection = detection.with_hbbs(&hbbs);
        let crop = calculate_crop_area(frame_width, frame_height, &detection, head_prob_threshold).unwrap();
        assert!(matches!(crop, CropResult::Single(_)));
    }

    #[test]
    fn test_is_within_percentage() {
        // Test identical values
        let crop1 = CropArea::new(100.0, 100.0, 200.0, 200.0);
        let crop2 = CropArea::new(100.0, 100.0, 200.0, 200.0);
        assert!(crop1.is_within_percentage(&crop2, 5.0));

        // Test values within 5%
        let crop1 = CropArea::new(100.0, 100.0, 200.0, 200.0);
        let crop2 = CropArea::new(102.0, 98.0, 204.0, 196.0); // All values within 2%
        assert!(crop1.is_within_percentage(&crop2, 5.0));

        // Test values exactly at 5% threshold
        let crop1 = CropArea::new(100.0, 100.0, 200.0, 200.0);
        let crop2 = CropArea::new(104.99, 95.01, 209.99, 190.01); // All values just under 5%
        assert!(crop1.is_within_percentage(&crop2, 5.0));

        // Test values just over 5%
        let crop1 = CropArea::new(100.0, 100.0, 200.0, 200.0);
        let crop2 = CropArea::new(106.0, 94.0, 211.0, 189.0); // All values just over 5%
        assert!(!crop1.is_within_percentage(&crop2, 5.0));

        // Test with zero values
        let crop1 = CropArea::new(0.0, 0.0, 200.0, 200.0);
        let crop2 = CropArea::new(0.0, 0.0, 200.0, 200.0);
        assert!(crop1.is_within_percentage(&crop2, 5.0));

        // Test with one zero value
        let crop1 = CropArea::new(0.0, 100.0, 200.0, 200.0);
        let crop2 = CropArea::new(1.0, 100.0, 200.0, 200.0);
        assert!(!crop1.is_within_percentage(&crop2, 5.0));

        // Test with very small values
        let crop1 = CropArea::new(1.0, 1.0, 1.0, 1.0);
        let crop2 = CropArea::new(1.05, 0.95, 1.05, 0.95); // All values within 5%
        assert!(crop1.is_within_percentage(&crop2, 5.0));

        // Test with very large values
        let crop1 = CropArea::new(1000.0, 1000.0, 2000.0, 2000.0);
        let crop2 = CropArea::new(1050.0, 950.0, 2100.0, 1900.0); // All values within 5%
        assert!(crop1.is_within_percentage(&crop2, 5.0));

        // Test with different threshold values
        let crop1 = CropArea::new(100.0, 100.0, 200.0, 200.0);
        let crop2 = CropArea::new(110.0, 90.0, 220.0, 180.0); // All values at 10%
        assert!(!crop1.is_within_percentage(&crop2, 5.0));
        assert!(crop1.is_within_percentage(&crop2, 10.0));
        assert!(crop1.is_within_percentage(&crop2, 15.0));

        // Test with mixed differences
        let crop1 = CropArea::new(100.0, 100.0, 200.0, 200.0);
        let crop2 = CropArea::new(102.0, 98.0, 210.0, 190.0); // x,y within 2%, width,height at 5%
        assert!(crop1.is_within_percentage(&crop2, 5.0));
        assert!(!crop1.is_within_percentage(&crop2, 4.0));
    }
} 