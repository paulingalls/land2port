use anyhow::Result;
use usls::Hbb;

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
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Checks if this crop area is within the specified percentage of another crop area
    ///
    /// # Arguments
    /// * `other` - The other crop area to compare against
    /// * `frame_width` - The width of the frame
    /// * `threshold_percent` - The maximum allowed difference as a percentage (e.g. 5.0 for 5%)
    ///
    /// # Returns
    /// `true` if the x and width are within the threshold percentage of each other
    pub fn is_within_percentage(
        &self,
        other: &CropArea,
        frame_width: f32,
        threshold_percent: f32,
    ) -> bool {
        let threshold = threshold_percent / 100.0;

        // Helper function to check if two values are within threshold percentage of each other
        let is_within_threshold = |_label: &str, a: f32, b: f32| -> bool {
            let diff = (a - b).abs();
            let percent = diff / frame_width;
            percent <= threshold + f32::EPSILON
        };

        let x_ok = is_within_threshold("x", self.x, other.x);
        let y_ok = is_within_threshold("y", self.y, other.y);
        let w_ok = is_within_threshold("width", self.width, other.width);
        let h_ok = is_within_threshold("height", self.height, other.height);
        x_ok && y_ok && w_ok && h_ok
    }
}

// Helper utilities to reduce duplication across crop calculations
fn compute_three_four_width(frame_height: f32) -> f32 {
    frame_height * (3.0 / 4.0)
}

fn clamp_x_for_width(x: f32, width: f32, frame_width: f32) -> f32 {
    if x < 0.0 {
        0.0
    } else if x + width > frame_width {
        frame_width - width
    } else {
        x
    }
}

fn make_single_crop_centered(center_x: f32, frame_width: f32, frame_height: f32) -> CropArea {
    let height = frame_height;
    let width = compute_three_four_width(frame_height);
    let x = clamp_x_for_width(center_x - width / 2.0, width, frame_width);
    CropArea::new(x, 0.0, width, height)
}

fn center_x_of_bbox(bbox: &CropArea) -> f32 {
    bbox.x + bbox.width / 2.0
}

fn half_stack_dims(frame_width: f32, frame_height: f32) -> (f32, f32, f32) {
    let crop_width = frame_width * 0.5;
    let crop_height = crop_width * (8.0 / 9.0);
    let default_y = (frame_height - crop_height) / 2.0;
    (crop_width, crop_height, default_y)
}

fn vertical_y_for_heads(
    heads: &[&Hbb],
    default_y: f32,
    frame_height: f32,
    crop_height: f32,
) -> f32 {
    if heads.is_empty() {
        return default_y;
    }
    let group_top = heads.iter().map(|h| h.ymin()).fold(f32::MAX, f32::min);
    let group_bottom = heads.iter().map(|h| h.ymax()).fold(f32::MIN, f32::max);
    if group_top < default_y {
        0.0
    } else if group_bottom > default_y + crop_height {
        frame_height - crop_height
    } else {
        default_y
    }
}

/// Represents the result of calculating crop areas
#[derive(Debug, Clone)]
pub enum CropResult {
    /// A single crop area
    Single(CropArea),
    /// Two crop areas that should be stacked vertically
    Stacked(CropArea, CropArea),
    /// Resize the entire frame (for graphic mode)
    Resize(CropArea),
}

/// Calculates crop area when no heads are detected
pub fn calculate_no_heads_crop(
    frame_width: f32,
    frame_height: f32,
    is_graphic: bool,
) -> CropResult {
    if is_graphic {
        // For graphic mode, return a resize crop that covers the entire frame
        CropResult::Resize(CropArea::new(0.0, 0.0, frame_width, frame_height))
    } else {
        // For no heads, center a 3:4 crop on the frame center
        let center_x = frame_width / 2.0;
        CropResult::Single(make_single_crop_centered(
            center_x,
            frame_width,
            frame_height,
        ))
    }
}

/// Calculates crop area for a single head
pub fn calculate_single_head_crop(frame_width: f32, frame_height: f32, head: &Hbb) -> CropResult {
    CropResult::Single(make_single_crop_centered(
        head.cx(),
        frame_width,
        frame_height,
    ))
}

/// Calculates crop area for two heads
pub fn calculate_two_heads_crop(
    use_stack_crop: bool,
    frame_width: f32,
    frame_height: f32,
    head1: &Hbb,
    head2: &Hbb,
) -> CropResult {
    // Calculate the bounding box of the two heads
    let bbox = calculate_bounding_box(&[head1, head2]);

    // Check if the width of the bounding box is less than or equal to 3/4 of the frame height
    if bbox.width <= frame_height * 0.75 {
        // Return a single crop centered on the bounding box
        let center_x = center_x_of_bbox(&bbox);
        CropResult::Single(make_single_crop_centered(
            center_x,
            frame_width,
            frame_height,
        ))
    } else if use_stack_crop {
        // Return two crops with specific dimensions and positions
        let (crop_width, crop_height, default_y) = half_stack_dims(frame_width, frame_height);

        let (left_head, right_head) = if head1.cx() <= head2.cx() {
            (head1, head2)
        } else {
            (head2, head1)
        };

        // Determine vertical placement for each side
        let crop1_y = vertical_y_for_heads(&[left_head], default_y, frame_height, crop_height);
        let crop2_y = vertical_y_for_heads(&[right_head], default_y, frame_height, crop_height);

        // Calculate default x positions
        let mut crop1_x = 0.0;
        let mut crop2_x = crop_width;

        // Calculate how much of each head is in each crop with default positions
        let left_head_in_crop1 =
            (left_head.xmax().min(crop1_x + crop_width) - left_head.xmin().max(crop1_x)).max(0.0);
        let left_head_in_crop2 =
            (left_head.xmax().min(crop2_x + crop_width) - left_head.xmin().max(crop2_x)).max(0.0);
        let right_head_in_crop1 =
            (right_head.xmax().min(crop1_x + crop_width) - right_head.xmin().max(crop1_x)).max(0.0);
        let right_head_in_crop2 =
            (right_head.xmax().min(crop2_x + crop_width) - right_head.xmin().max(crop2_x)).max(0.0);

        // Check if either head spans both crops
        let left_head_spans = left_head_in_crop1 > 0.0 && left_head_in_crop2 > 0.0;
        let right_head_spans = right_head_in_crop1 > 0.0 && right_head_in_crop2 > 0.0;

        if left_head_spans || right_head_spans {
            // Default positions
            crop1_x = 0.0;
            crop2_x = crop_width;

            // Nudge crop1 right if needed to fully contain the left head
            if left_head.xmax() > crop1_x + crop_width {
                crop1_x = left_head.xmax() - crop_width;
            }
            if left_head.xmin() < crop1_x {
                crop1_x = left_head.xmin();
            }
            crop1_x = crop1_x.max(0.0).min(crop_width);

            // Nudge crop2 left if needed to fully contain the right head
            if right_head.xmin() < crop2_x {
                crop2_x = right_head.xmin();
            }
            if right_head.xmax() > crop2_x + crop_width {
                crop2_x = right_head.xmax() - crop_width;
            }
            crop2_x = crop2_x.max(0.0).min(crop_width);
        }

        // First crop
        let crop1 = CropArea::new(crop1_x, crop1_y, crop_width, crop_height);

        // Second crop
        let crop2 = CropArea::new(crop2_x, crop2_y, crop_width, crop_height);

        CropResult::Stacked(crop1, crop2)
    } else {
        calculate_crop_from_largest_head(frame_width, frame_height, &[head1, head2])
    }
}

/// Calculates crop area for three heads
pub fn calculate_three_heads_crop(
    use_stack_crop: bool,
    frame_width: f32,
    frame_height: f32,
    heads: &[&Hbb],
) -> CropResult {
    // Check if heads are roughly the same size
    let areas: Vec<f32> = heads.iter().map(|h| h.width() * h.height()).collect();
    let min_area = areas.iter().fold(f32::MAX, |a, &b| a.min(b));
    let max_area = areas.iter().fold(f32::MIN, |a, &b| a.max(b));
    let size_ratio = max_area / min_area;
    let similar_size = size_ratio <= 2.5;

    // Check if heads are roughly equally spaced across the screen
    let centers: Vec<f32> = heads.iter().map(|h| h.cx()).collect();
    let sorted_centers = {
        let mut centers = centers.clone();
        centers.sort_by(|a, b| a.partial_cmp(b).unwrap());
        centers
    };

    let spacing1 = sorted_centers[1] - sorted_centers[0];
    let spacing2 = sorted_centers[2] - sorted_centers[1];
    let spacing_ratio = spacing1.max(spacing2) / spacing1.min(spacing2);
    let equally_spaced = spacing_ratio <= 2.0;

    if similar_size && equally_spaced && use_stack_crop {
        // Create special stacked crop designed to work together for 9:16 final image
        // First crop: for two heads - 9:6 aspect ratio (will be top portion of 9:16)
        let crop1_height = frame_height * 0.8; // 80% of frame height
        let crop1_width = crop1_height * 1.5; // 9:6 aspect ratio
        let crop1_y = frame_height * 0.1; // 10% from top

        // Second crop: for single head - 9:10 aspect ratio (will be bottom portion of 9:16)
        let crop2_height = frame_height * 0.8; // 80% of frame height
        let crop2_width = crop2_height * 0.9; // 9:10 aspect ratio
        let crop2_y = frame_height * 0.15; // 15% from top

        // Position first crop to contain the leftmost two heads
        // Calculate the bounding box of the leftmost and middle heads
        let leftmost_center = sorted_centers[0];
        let middle_center = sorted_centers[1];

        // Find the heads that correspond to these centers
        let head1 = heads
            .iter()
            .find(|h| (h.cx() - leftmost_center).abs() < 1.0)
            .unwrap();
        let head2 = heads
            .iter()
            .find(|h| (h.cx() - middle_center).abs() < 1.0)
            .unwrap();

        // Calculate the bounding box of these two heads
        let min_x = head1.xmin().min(head2.xmin());
        let max_x = head1.xmax().max(head2.xmax());
        let center_between_two = (min_x + max_x) / 2.0;

        let mut crop1_x = center_between_two - crop1_width / 2.0;
        crop1_x = crop1_x.max(0.0).min(frame_width - crop1_width);

        // Position second crop to contain the rightmost head
        let rightmost_center = sorted_centers[2];
        let mut crop2_x = rightmost_center - crop2_width / 2.0;
        crop2_x = crop2_x.max(0.0).min(frame_width - crop2_width);

        let crop1 = CropArea::new(crop1_x, crop1_y, crop1_width, crop1_height);
        let crop2 = CropArea::new(crop2_x, crop2_y, crop2_width, crop2_height);

        return CropResult::Stacked(crop1, crop2);
    }

    // Fall back to the existing logic
    calculate_four_and_five_heads_crop(use_stack_crop, frame_width, frame_height, heads)
}

/// Calculates crop area for four and five heads
pub fn calculate_four_and_five_heads_crop(
    use_stack_crop: bool,
    frame_width: f32,
    frame_height: f32,
    heads: &[&Hbb],
) -> CropResult {
    // Calculate the bounding box that contains all heads
    let bbox = calculate_bounding_box(heads);

    // If the bounding box width is less than or equal to 3/4 of the frame height,
    // we can fit all heads in a single crop
    if bbox.width <= frame_height * (3.0 / 4.0) {
        let center_x = center_x_of_bbox(&bbox);
        CropResult::Single(make_single_crop_centered(
            center_x,
            frame_width,
            frame_height,
        ))
    } else if use_stack_crop {
        // Mirror the two-heads stacked crop: two half-width crops side-by-side with 8:9 height
        let (crop_width, crop_height, default_y) = half_stack_dims(frame_width, frame_height);

        // Default crop positions
        let mut x1 = 0.0;
        let mut x2 = crop_width;
        let mut crop1_y = default_y;
        let mut crop2_y = default_y;
        let crop1_default = CropArea::new(x1, crop1_y, crop_width, crop_height);
        let crop2_default = CropArea::new(x2, crop2_y, crop_width, crop_height);

        // Check if all heads are fully contained in at least one default crop
        let all_heads_contained = heads.iter().all(|head| {
            let head_xmin = head.xmin();
            let head_xmax = head.xmax();
            let in_crop1 =
                head_xmin >= crop1_default.x && head_xmax <= crop1_default.x + crop1_default.width;
            let in_crop2 =
                head_xmin >= crop2_default.x && head_xmax <= crop2_default.x + crop2_default.width;
            in_crop1 || in_crop2
        });

        if all_heads_contained {
            // Vertically adjust crops while keeping default x positions
            let frame_center = frame_width / 2.0;
            let mut left_heads = Vec::new();
            let mut right_heads = Vec::new();
            for head in heads {
                if head.cx() < frame_center {
                    left_heads.push(*head);
                } else {
                    right_heads.push(*head);
                }
            }

            let left_y = vertical_y_for_heads(&left_heads, default_y, frame_height, crop_height);

            let right_y = vertical_y_for_heads(&right_heads, default_y, frame_height, crop_height);

            let crop1 = CropArea::new(0.0, left_y, crop_width, crop_height);
            let crop2 = CropArea::new(crop_width, right_y, crop_width, crop_height);
            return CropResult::Stacked(crop1, crop2);
        }

        // Assign heads to crops based on which side of the frame they're closer to
        let frame_center = frame_width / 2.0;
        let mut crop1_heads = Vec::new();
        let mut crop2_heads = Vec::new();
        for head in heads {
            if head.cx() < frame_center {
                crop1_heads.push(*head);
            } else {
                crop2_heads.push(*head);
            }
        }

        // Vertical positioning per side (top/bottom bias like two-heads)
        if !crop1_heads.is_empty() {
            crop1_y = vertical_y_for_heads(&crop1_heads, default_y, frame_height, crop_height);
        }

        if !crop2_heads.is_empty() {
            crop2_y = vertical_y_for_heads(&crop2_heads, default_y, frame_height, crop_height);
        }

        // Horizontal positioning to contain assigned heads
        if !crop1_heads.is_empty() {
            let min_x = crop1_heads
                .iter()
                .map(|h| h.xmin())
                .fold(f32::MAX, f32::min);
            let max_x = crop1_heads
                .iter()
                .map(|h| h.xmax())
                .fold(f32::MIN, f32::max);
            if max_x - min_x > crop_width {
                x1 = (min_x + max_x - crop_width) / 2.0;
            } else {
                x1 = min_x;
            }
            // Clamp within its half
            x1 = x1.max(0.0).min(crop_width);
        }

        if !crop2_heads.is_empty() {
            let min_x = crop2_heads
                .iter()
                .map(|h| h.xmin())
                .fold(f32::MAX, f32::min);
            let max_x = crop2_heads
                .iter()
                .map(|h| h.xmax())
                .fold(f32::MIN, f32::max);
            if max_x - min_x > crop_width {
                x2 = (min_x + max_x - crop_width) / 2.0;
            } else {
                x2 = max_x - crop_width;
            }
            // Clamp within its half start position
            x2 = x2.max(0.0).min(crop_width);
        }

        // Create the crops
        let mut crop1 = CropArea::new(x1, crop1_y, crop_width, crop_height);
        let mut crop2 = CropArea::new(x2, crop2_y, crop_width, crop_height);

        // Verify that every head is fully contained in at least one crop, adjust if not
        for head in heads {
            let head_xmin = head.xmin();
            let head_xmax = head.xmax();
            let head_center = head.cx();
            let in_crop1 = head_xmin >= crop1.x && head_xmax <= crop1.x + crop1.width;
            let in_crop2 = head_xmin >= crop2.x && head_xmax <= crop2.x + crop2.width;
            if !in_crop1 && !in_crop2 {
                let dist_to_crop1 = (head_center - (crop1.x + crop1.width / 2.0)).abs();
                let dist_to_crop2 = (head_center - (crop2.x + crop2.width / 2.0)).abs();
                if dist_to_crop1 <= dist_to_crop2 {
                    let new_x1 = head_xmin;
                    x1 = new_x1.max(0.0).min(crop_width);
                    crop1 = CropArea::new(x1, crop1_y, crop_width, crop_height);
                } else {
                    let new_x2 = head_xmax - crop_width;
                    x2 = new_x2.max(0.0).min(crop_width);
                    crop2 = CropArea::new(x2, crop2_y, crop_width, crop_height);
                }
            }
        }
        CropResult::Stacked(crop1, crop2)
    } else {
        calculate_crop_from_largest_head(frame_width, frame_height, heads)
    }
}

/// Calculates crop area for six or more heads
pub fn calculate_six_or_more_heads_crop(
    use_stack_crop: bool,
    frame_width: f32,
    frame_height: f32,
    heads: &[&Hbb],
) -> CropResult {
    // Calculate the bounding box that contains all heads
    let bbox = calculate_bounding_box(heads);

    // Check if the bounding box width is less than or equal to 3/4 of the frame height
    if bbox.width <= frame_height * (3.0 / 4.0) {
        let center_x = center_x_of_bbox(&bbox);
        CropResult::Single(make_single_crop_centered(
            center_x,
            frame_width,
            frame_height,
        ))
    } else {
        let head_areas: Vec<f32> = heads.iter().map(|h| h.area()).collect();

        // Find a head that is at least 2x larger than all other heads
        let mut large_head_index = None;
        for (i, &area) in head_areas.iter().enumerate() {
            let is_large = head_areas
                .iter()
                .enumerate()
                .all(|(j, &other_area)| i == j || area >= other_area * 2.5);
            if is_large {
                large_head_index = Some(i);
                break;
            }
        }

        if let Some(large_head_idx) = large_head_index {
            let large_head = heads[large_head_idx];

            if use_stack_crop {
                // Two stacked crops mirroring two-heads behavior (half-width 8:9, vertically centered)
                let (crop_width, crop_height, crop_y) = half_stack_dims(frame_width, frame_height);

                // First crop centered on the large head
                let mut crop1_x = large_head.cx() - crop_width / 2.0;
                crop1_x = crop1_x.max(0.0).min(frame_width - crop_width);
                let crop1 = CropArea::new(crop1_x, crop_y, crop_width, crop_height);

                // Second crop for remaining heads
                let remaining_heads: Vec<&Hbb> = heads
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != large_head_idx)
                    .map(|(_, &head)| head)
                    .collect();

                if remaining_heads.is_empty() {
                    return CropResult::Single(CropArea::new(
                        crop1_x,
                        0.0,
                        crop_width,
                        frame_height,
                    ));
                }

                // Position second crop to contain remaining heads near their center
                let remaining_bbox = calculate_bounding_box(&remaining_heads);
                let mut crop2_x = center_x_of_bbox(&remaining_bbox) - crop_width / 2.0;
                crop2_x = crop2_x.max(0.0).min(frame_width - crop_width);

                // Ensure crops don't overlap too much
                if (crop1_x - crop2_x).abs() < crop_width * 0.5 {
                    // If crops would overlap significantly, position second crop at the opposite side
                    if crop1_x < frame_width / 2.0 {
                        crop2_x = frame_width - crop_width;
                    } else {
                        crop2_x = 0.0;
                    }
                }

                let crop2 = CropArea::new(crop2_x, crop_y, crop_width, crop_height);

                CropResult::Stacked(crop1, crop2)
            } else {
                // Just center a single crop on the large head
                calculate_single_head_crop(frame_width, frame_height, large_head)
            }
        } else {
            // No large head found, call calculate_no_heads_crop with is_graphic = false
            calculate_no_heads_crop(frame_width, frame_height, false)
        }
    }
}

/// Calculates crop area from the largest head
pub fn calculate_crop_from_largest_head(
    frame_width: f32,
    frame_height: f32,
    heads: &[&Hbb],
) -> CropResult {
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
        x = 0.0; // Clamp to left edge
    } else if x + width > frame_width {
        x = frame_width - width; // Clamp to right edge
    }

    CropResult::Single(CropArea::new(x, 0.0, width, height))
}

/// Calculates the optimal crop area based on detected heads
///
/// # Arguments
/// * `use_stack_crop` - Whether the function can return a stacked crop result
/// * `is_graphic` - Whether this is for graphic mode (affects no heads case)
/// * `frame_width` - Width of the input frame
/// * `frame_height` - Height of the input frame
/// * `heads` - Vector of head detections that have already been filtered by confidence threshold
pub fn calculate_crop_area(
    use_stack_crop: bool,
    is_graphic: bool,
    frame_width: f32,
    frame_height: f32,
    heads: &[&Hbb],
) -> Result<CropResult> {
    match heads.len() {
        0 => Ok(calculate_no_heads_crop(
            frame_width,
            frame_height,
            is_graphic,
        )),
        1 => Ok(calculate_single_head_crop(
            frame_width,
            frame_height,
            heads[0],
        )),
        2 => Ok(calculate_two_heads_crop(
            use_stack_crop,
            frame_width,
            frame_height,
            heads[0],
            heads[1],
        )),
        3 => Ok(calculate_three_heads_crop(
            use_stack_crop,
            frame_width,
            frame_height,
            heads,
        )),
        4..=5 => Ok(calculate_four_and_five_heads_crop(
            use_stack_crop,
            frame_width,
            frame_height,
            heads,
        )),
        6.. => Ok(calculate_six_or_more_heads_crop(
            use_stack_crop,
            frame_width,
            frame_height,
            heads,
        )),
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

    CropArea::new(min_x, min_y, max_x - min_x, max_y - min_y)
}

/// Determines if two head counts would result in different crop classes
///
/// Crop classes are defined as:
/// - 0 heads
/// - 1 head  
/// - 2 heads
/// - 3 to 5 heads
/// - More than 5 heads
///
/// # Arguments
/// * `head_count1` - First head count
/// * `head_count2` - Second head count
///
/// # Returns
/// `true` if the head counts would result in different crop classes, `false` otherwise
pub fn is_crop_class_same(head_count1: usize, head_count2: usize) -> bool {
    // Helper function to get crop class for a given head count
    fn get_crop_class(head_count: usize) -> u8 {
        match head_count {
            0 => 0,   // 0 heads
            1 => 1,   // 1 head
            2 => 2,   // 2 heads
            3 => 3,   // 3 heads
            4.. => 4, // 4 or more heads
        }
    }

    get_crop_class(head_count1) == get_crop_class(head_count2)
}

/// Checks if two crop results are similar based on a threshold percentage
pub fn is_crop_similar(crop1: &CropResult, crop2: &CropResult, width: f32, threshold: f32) -> bool {
    match (crop1, crop2) {
        (CropResult::Single(crop1), CropResult::Single(crop2)) => {
            crop1.is_within_percentage(crop2, width, threshold)
        }
        (CropResult::Stacked(crop1_1, crop1_2), CropResult::Stacked(crop2_1, crop2_2)) => {
            crop1_1.is_within_percentage(crop2_1, width, threshold)
                && crop1_2.is_within_percentage(crop2_2, width, threshold)
        }
        (CropResult::Resize(crop1), CropResult::Resize(crop2)) => {
            crop1.is_within_percentage(crop2, width, threshold)
        }
        _ => false, // If crop types don't match, use the new crop
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        // test two heads with one at right edge
        let head1 = Hbb::from_xyxy(1063.6982, 335.45892, 1262.3218, 646.60675);
        let head2 = Hbb::from_xyxy(1846.0652, 228.14204, 1919.9954, 533.70746);
        let bbox = calculate_bounding_box(&[&head1, &head2]);
        assert!((bbox.x - 1063.6982).abs() < 1.0);
        assert!((bbox.y - 228.14204).abs() < 1.0);
        assert!((bbox.width - 856.2972).abs() < 1.0);
        assert!((bbox.height - 418.46471).abs() < 1.0);

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

        let crop = calculate_no_heads_crop(frame_width, frame_height, false);

        match crop {
            CropResult::Single(crop) => {
                // Height should match frame height
                assert!((crop.height - frame_height).abs() < 1.0);

                // Width should be 3/4 of the height (3:4 aspect ratio)
                let expected_width = frame_height * (3.0 / 4.0);
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
    fn test_calculate_no_heads_crop_graphic() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;

        let crop = calculate_no_heads_crop(frame_width, frame_height, true);

        match crop {
            CropResult::Resize(crop) => {
                // Should cover the entire frame
                assert_eq!(crop.x, 0.0);
                assert_eq!(crop.y, 0.0);
                assert_eq!(crop.width, frame_width);
                assert_eq!(crop.height, frame_height);
            }
            _ => panic!("Expected resize crop for graphic mode"),
        }
    }

    #[test]
    fn test_calculate_single_head_crop() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;

        // Test centered head
        let head = Hbb::from_cxcywh(frame_width / 2.0, frame_height / 2.0, 100.0, 100.0);
        let crop = calculate_single_head_crop(frame_width, frame_height, &head);

        match crop {
            CropResult::Single(crop) => {
                // Height should match frame height
                assert!((crop.height - frame_height).abs() < 1.0);

                // Width should be 3/4 of the height (3:4 aspect ratio)
                let expected_width = frame_height * (3.0 / 4.0);
                assert!((crop.width - expected_width).abs() < 1.0);

                // Should be centered on the head's x-coordinate
                assert!((crop.x + crop.width / 2.0 - frame_width / 2.0).abs() < 1.0);

                // Should be within frame bounds
                assert!(crop.x >= 0.0);
                assert!(crop.y >= 0.0);
                assert!(crop.x + crop.width <= frame_width);
                assert!(crop.y + crop.height <= frame_height);
            }
            _ => panic!("Expected single crop for single head case"),
        }

        // Test head on far left
        let head = Hbb::from_cxcywh(50.0, frame_height / 2.0, 100.0, 100.0);
        let crop = calculate_single_head_crop(frame_width, frame_height, &head);

        match crop {
            CropResult::Single(crop) => {
                // Height should match frame height
                assert!((crop.height - frame_height).abs() < 1.0);

                // Width should be 3/4 of the height (3:4 aspect ratio)
                let expected_width = frame_height * (3.0 / 4.0);
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
        let head = Hbb::from_cxcywh(frame_width - 50.0, frame_height / 2.0, 100.0, 100.0);
        let crop = calculate_single_head_crop(frame_width, frame_height, &head);

        match crop {
            CropResult::Single(crop) => {
                // Height should match frame height
                assert!((crop.height - frame_height).abs() < 1.0);

                // Width should be 3/4 of the height (3:4 aspect ratio)
                let expected_width = frame_height * (3.0 / 4.0);
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
        let crop = calculate_two_heads_crop(true, frame_width, frame_height, &head1, &head2);

        match crop {
            CropResult::Single(crop) => {
                // Height should match frame height
                assert!((crop.height - frame_height).abs() < 1.0);

                // Width should be 3/4 of the height
                let expected_width = frame_height * (3.0 / 4.0);
                assert!((crop.width - expected_width).abs() < 1.0);

                // Calculate the center of the bounding box
                let bbox_center_x = 425.0;

                // Should be centered on the bounding box center
                assert!((crop.x + crop.width / 2.0 - bbox_center_x).abs() < 1.0);

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
        let head1 = Hbb::from_cxcywh(frame_width / 4.0, frame_height / 2.0, 100.0, 100.0);
        let head2 = Hbb::from_cxcywh(3.0 * frame_width / 4.0, frame_height / 2.0, 100.0, 100.0);
        let crop = calculate_two_heads_crop(true, frame_width, frame_height, &head1, &head2);

        match crop {
            CropResult::Stacked(crop1, crop2) => {
                // Both crops should have width equal to half frame width
                let expected_width = frame_width * 0.5;
                assert!((crop1.width - expected_width).abs() < 1.0);
                assert!((crop2.width - expected_width).abs() < 1.0);

                // Both crops should have height of 8/9 of the crop width
                let expected_height = expected_width * (8.0 / 9.0);
                assert!((crop1.height - expected_height).abs() < 1.0);
                assert!((crop2.height - expected_height).abs() < 1.0);

                // First crop should be at x=0
                assert!(crop1.x.abs() < 1.0);

                // Second crop should be at x = frame_width/2
                assert!((crop2.x - (frame_width / 2.0)).abs() < 1.0);

                // Both crops should be centered vertically
                let expected_y = (frame_height - expected_height) / 2.0;
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
        let head1 = Hbb::from_cxcywh(frame_width / 4.0, 50.0, 100.0, 100.0); // Head near top
        let head2 = Hbb::from_cxcywh(3.0 * frame_width / 4.0, frame_height - 50.0, 100.0, 100.0); // Head near bottom
        let crop = calculate_two_heads_crop(true, frame_width, frame_height, &head1, &head2);

        match crop {
            CropResult::Stacked(crop1, crop2) => {
                // Both crops should have width equal to half frame width
                let expected_width = frame_width * 0.5;
                assert!((crop1.width - expected_width).abs() < 1.0);
                assert!((crop2.width - expected_width).abs() < 1.0);

                // Both crops should have height of 8/9 of the crop width
                let expected_height = expected_width * (8.0 / 9.0);
                assert!((crop1.height - expected_height).abs() < 1.0);
                assert!((crop2.height - expected_height).abs() < 1.0);

                // First crop should be at x=0
                assert!(crop1.x.abs() < 1.0);

                // Second crop should be at x = frame_width/2
                assert!((crop2.x - (frame_width / 2.0)).abs() < 1.0);

                // First crop should be at y=0 since head1 is near the top
                assert!(crop1.y.abs() < 1.0);

                // Second crop should be at y = frame_height - expected_height since head2 is near the bottom
                assert!((crop2.y - (frame_height - expected_height)).abs() < 1.0);

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
        let head1 = Hbb::from_cxcywh(frame_width / 2.0, frame_height / 2.0, 400.0, 100.0);
        // Second head is far to the right, ensuring the bounding box is wider than 3/4 of frame height
        let head2 = Hbb::from_cxcywh(frame_width - 200.0, frame_height / 2.0, 100.0, 100.0);

        let crop = calculate_two_heads_crop(true, frame_width, frame_height, &head1, &head2);

        match crop {
            CropResult::Stacked(crop1, crop2) => {
                // Both crops should have width equal to half frame width
                let expected_width = frame_width * 0.5;
                assert!((crop1.width - expected_width).abs() < 1.0);
                assert!((crop2.width - expected_width).abs() < 1.0);

                // Both crops should have height of 8/9 of the crop width
                let expected_height = expected_width * (8.0 / 9.0);
                assert!((crop1.height - expected_height).abs() < 1.0);
                assert!((crop2.height - expected_height).abs() < 1.0);

                // Both crops should be centered vertically
                let expected_y = (frame_height - expected_height) / 2.0;
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
    fn test_calculate_two_heads_crop_specific_heads() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;

        // Test with the specific heads provided
        let head1 = Hbb::from_xyxy(1063.6982, 335.45892, 1262.3218, 646.60675);
        let head2 = Hbb::from_xyxy(1846.0652, 228.14204, 1919.9954, 533.70746);

        let crop = calculate_two_heads_crop(true, frame_width, frame_height, &head1, &head2);

        match crop {
            CropResult::Stacked(crop1, crop2) => {
                // Both crops should have width equal to half frame width
                let expected_width = frame_width * 0.5;
                assert!((crop1.width - expected_width).abs() < 1.0);
                assert!((crop2.width - expected_width).abs() < 1.0);

                // Both crops should have height of 8/9 of the crop width
                let expected_height = expected_width * (8.0 / 9.0);
                assert!((crop1.height - expected_height).abs() < 1.0);
                assert!((crop2.height - expected_height).abs() < 1.0);

                // Both crops should be within frame bounds
                assert!(crop1.x >= 0.0);
                assert!(crop1.y >= 0.0);
                assert!(crop1.x + crop1.width <= frame_width);
                assert!(crop1.y + crop1.height <= frame_height);
                assert!(crop2.x >= 0.0);
                assert!(crop2.y >= 0.0);
                assert!(crop2.x + crop2.width <= frame_width);
                assert!(crop2.y + crop2.height <= frame_height);

                // Check that head1 is fully contained in at least one crop
                let head1_xmin = head1.xmin();
                let head1_xmax = head1.xmax();
                let head1_in_crop1 = head1_xmin >= crop1.x && head1_xmax <= crop1.x + crop1.width;
                let head1_in_crop2 = head1_xmin >= crop2.x && head1_xmax <= crop2.x + crop2.width;

                // Head1 should be fully contained in at least one crop
                assert!(
                    head1_in_crop1 || head1_in_crop2,
                    "Head1 should be fully contained in at least one crop"
                );

                // Check that head2 is fully contained in at least one crop
                let head2_xmin = head2.xmin();
                let head2_xmax = head2.xmax();
                let head2_in_crop1 = head2_xmin >= crop1.x && head2_xmax <= crop1.x + crop1.width;
                let head2_in_crop2 = head2_xmin >= crop2.x && head2_xmax <= crop2.x + crop2.width;

                // Head2 should be fully contained in at least one crop
                assert!(
                    head2_in_crop1 || head2_in_crop2,
                    "Head2 should be fully contained in at least one crop"
                );
            }
            _ => panic!("Expected stacked crops for far heads case"),
        }
    }

    #[test]
    fn test_calculate_four_and_five_heads_crop_close() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;

        // Create three heads that are close together (within 3/4 of frame height)
        let head1 = Hbb::from_cxcywh(frame_width / 2.0 - 100.0, frame_height / 2.0, 100.0, 100.0);
        let head2 = Hbb::from_cxcywh(frame_width / 2.0, frame_height / 2.0, 100.0, 100.0);
        let head3 = Hbb::from_cxcywh(frame_width / 2.0 + 100.0, frame_height / 2.0, 100.0, 100.0);
        let heads = vec![&head1, &head2, &head3];

        let crop = calculate_four_and_five_heads_crop(true, frame_width, frame_height, &heads);

        match crop {
            CropResult::Single(crop) => {
                // Height should match frame height
                assert!((crop.height - frame_height).abs() < 1.0);

                // Width should be 3/4 of the height
                let expected_width = frame_height * (3.0 / 4.0);
                assert!((crop.width - expected_width).abs() < 1.0);

                // Calculate the bounding box of all heads
                let bbox = calculate_bounding_box(&heads);
                let bbox_center_x = bbox.x + bbox.width / 2.0;

                // Crop should be centered on the bounding box center
                assert!((crop.x + crop.width / 2.0 - bbox_center_x).abs() < 1.0);

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
    fn test_calculate_four_and_five_heads_crop_far_default() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;

        // Create three heads that are far apart horizontally, but default crop positions are sufficient
        let head1 = Hbb::from_cxcywh(200.0, frame_height / 2.0, 100.0, 100.0);
        let head2 = Hbb::from_cxcywh(1200.0, frame_height / 2.0, 100.0, 100.0);
        let head3 = Hbb::from_cxcywh(1800.0, frame_height / 2.0, 100.0, 100.0);
        let heads = vec![&head1, &head2, &head3];

        let crop = calculate_four_and_five_heads_crop(true, frame_width, frame_height, &heads);

        match crop {
            CropResult::Stacked(crop1, crop2) => {
                // Both crops should have height of 8/9 of half frame width
                let expected_width = frame_width * 0.5;
                let expected_height = expected_width * (8.0 / 9.0);
                assert!((crop1.height - expected_height).abs() < 1.0);
                assert!((crop2.height - expected_height).abs() < 1.0);

                // Both crops should have width equal to half frame width
                assert!((crop1.width - expected_width).abs() < 1.0);
                assert!((crop2.width - expected_width).abs() < 1.0);

                // Both crops should be centered vertically
                let expected_y = (frame_height - expected_height) / 2.0;
                assert!((crop1.y - expected_y).abs() < 1.0);
                assert!((crop2.y - expected_y).abs() < 1.0);

                // First crop should be at x=0
                assert!(crop1.x.abs() < 1.0);

                // Second crop should be at x = frame_width/2
                assert!((crop2.x - (frame_width / 2.0)).abs() < 1.0);

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
                    assert!(
                        in_crop1 || in_crop2,
                        "Head should be fully contained in at least one crop"
                    );
                }
            }
            _ => panic!("Expected stacked crops for far heads case"),
        }
    }

    #[test]
    fn test_calculate_four_and_five_heads_crop_all_heads_contained_with_vertical_adjust() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;

        // Arrange heads so that all are horizontally contained within default half crops
        // Left side heads near top; Right side heads near bottom
        let head_left_top = Hbb::from_cxcywh(200.0, 60.0, 100.0, 100.0);
        let head_left_top2 = Hbb::from_cxcywh(400.0, 65.0, 100.0, 100.0);
        let head_right_bottom = Hbb::from_cxcywh(1520.0, frame_height - 60.0, 100.0, 100.0);
        let head_right_bottom2 = Hbb::from_cxcywh(1720.0, frame_height - 65.0, 100.0, 100.0);
        let heads = vec![
            &head_left_top,
            &head_left_top2,
            &head_right_bottom,
            &head_right_bottom2,
        ];

        let crop = calculate_four_and_five_heads_crop(true, frame_width, frame_height, &heads);

        match crop {
            CropResult::Stacked(crop1, crop2) => {
                let expected_width = frame_width * 0.5;
                let expected_height = expected_width * (8.0 / 9.0);

                // Left crop should be nudged to top (0.0)
                assert!((crop1.y - 0.0).abs() < 1.0);
                // Right crop should be nudged to bottom
                assert!((crop2.y - (frame_height - expected_height)).abs() < 1.0);

                // Width/height remain half-width based
                assert!((crop1.width - expected_width).abs() < 1.0);
                assert!((crop2.width - expected_width).abs() < 1.0);
                assert!((crop1.height - expected_height).abs() < 1.0);
                assert!((crop2.height - expected_height).abs() < 1.0);

                // x positions remain default halves
                assert!(crop1.x.abs() < 1.0);
                assert!((crop2.x - expected_width).abs() < 1.0);
            }
            _ => panic!("Expected stacked crops for all_heads_contained with vertical adjust case"),
        }
    }

    #[test]
    fn test_calculate_four_and_five_heads_crop_far_adjusted() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;

        // Create three heads that are far apart horizontally, requiring crop positions to be adjusted
        let head1 = Hbb::from_cxcywh(400.0, frame_height / 2.0, 100.0, 100.0);
        let head2 = Hbb::from_cxcywh(1000.0, frame_height / 2.0, 200.0, 100.0);
        let head3 = Hbb::from_cxcywh(1600.0, frame_height / 2.0, 100.0, 100.0);
        let heads = vec![&head1, &head2, &head3];

        let crop = calculate_four_and_five_heads_crop(true, frame_width, frame_height, &heads);

        match crop {
            CropResult::Stacked(crop1, crop2) => {
                // Both crops should have height of 8/9 of half frame width
                let expected_width = frame_width * 0.5;
                let expected_height = expected_width * (8.0 / 9.0);
                assert!((crop1.height - expected_height).abs() < 1.0);
                assert!((crop2.height - expected_height).abs() < 1.0);

                // Both crops should have width equal to half frame width
                assert!((crop1.width - expected_width).abs() < 1.0);
                assert!((crop2.width - expected_width).abs() < 1.0);

                // Both crops should be centered vertically
                let expected_y = (frame_height - expected_height) / 2.0;
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
                    assert!(
                        in_crop1 || in_crop2,
                        "Head should be fully contained in at least one crop"
                    );
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
        let head1 = Hbb::from_cxcywh(frame_width / 6.0, frame_height / 6.0, 100.0, 100.0);
        let head2 = Hbb::from_cxcywh(frame_width / 3.0, frame_height / 3.0, 100.0, 100.0);
        let head3 = Hbb::from_cxcywh(frame_width / 2.0, frame_height / 2.0, 100.0, 100.0);
        let head4 = Hbb::from_cxcywh(
            2.0 * frame_width / 3.0,
            2.0 * frame_height / 3.0,
            100.0,
            100.0,
        );
        let head5 = Hbb::from_cxcywh(
            5.0 * frame_width / 6.0,
            5.0 * frame_height / 6.0,
            100.0,
            100.0,
        );
        let head6 = Hbb::from_cxcywh(frame_width - 100.0, frame_height - 100.0, 100.0, 100.0);
        let heads = vec![&head1, &head2, &head3, &head4, &head5, &head6];

        let crop = calculate_crop_from_largest_head(frame_width, frame_height, &heads);

        match crop {
            CropResult::Single(crop) => {
                // Find the largest head by area
                let largest_head = heads
                    .iter()
                    .max_by(|a, b| a.area().partial_cmp(&b.area()).unwrap())
                    .unwrap();
                let head_center_x = largest_head.cx();

                // Height should match frame height
                assert!((crop.height - frame_height).abs() < 1.0);

                // Width should be 3/4 of the height (3:4 aspect ratio)
                let expected_width = frame_height * (3.0 / 4.0);
                assert!((crop.width - expected_width).abs() < 1.0);

                // Should be centered on the largest head unless at the edge
                let eps = 1e-3;
                if crop.x.abs() > eps && (frame_width - (crop.x + crop.width)).abs() > eps {
                    assert!((crop.x + crop.width / 2.0 - head_center_x).abs() < 1.0);
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
        let head1 = Hbb::from_cxcywh(frame_width / 6.0, frame_height / 6.0, 100.0, 100.0);
        let head2 = Hbb::from_cxcywh(frame_width / 3.0, frame_height / 3.0, 100.0, 100.0);
        let head3 = Hbb::from_cxcywh(frame_width / 2.0, frame_height / 2.0, 100.0, 100.0);
        let head4 = Hbb::from_cxcywh(
            2.0 * frame_width / 3.0,
            2.0 * frame_height / 3.0,
            100.0,
            100.0,
        );
        let head5 = Hbb::from_cxcywh(
            5.0 * frame_width / 6.0,
            5.0 * frame_height / 6.0,
            100.0,
            100.0,
        );
        // Make head6 the largest and position it near the right edge
        let head6 = Hbb::from_cxcywh(frame_width - 250.0, frame_height / 2.0, 200.0, 200.0);
        let heads = vec![&head1, &head2, &head3, &head4, &head5, &head6];

        let crop = calculate_crop_from_largest_head(frame_width, frame_height, &heads);

        match crop {
            CropResult::Single(crop) => {
                // Height should match frame height
                assert!((crop.height - frame_height).abs() < 1.0);

                // Width should be 3/4 of the height (3:4 aspect ratio)
                let expected_width = frame_height * (3.0 / 4.0);
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

        // Test no heads
        let heads: Vec<&Hbb> = vec![];
        let crop = calculate_crop_area(true, false, frame_width, frame_height, &heads).unwrap();
        assert!(matches!(crop, CropResult::Single(_)));

        // Test single head
        let head = Hbb::from_cxcywh(frame_width / 2.0, frame_height / 2.0, 100.0, 100.0)
            .with_confidence(0.9);
        let hbbs = vec![&head];
        let crop = calculate_crop_area(true, false, frame_width, frame_height, &hbbs).unwrap();
        assert!(matches!(crop, CropResult::Single(_)));

        // Test two heads
        let head1 = Hbb::from_cxcywh(frame_width / 4.0, frame_height / 2.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head2 = Hbb::from_cxcywh(3.0 * frame_width / 4.0, frame_height / 2.0, 100.0, 100.0)
            .with_confidence(0.9);
        let hbbs = vec![&head1, &head2];
        let crop = calculate_crop_area(true, false, frame_width, frame_height, &hbbs).unwrap();
        assert!(matches!(crop, CropResult::Stacked(_, _)));

        // Test three heads
        let head1 = Hbb::from_cxcywh(frame_width / 4.0, frame_height / 4.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head2 = Hbb::from_cxcywh(frame_width / 2.0, frame_height / 2.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head3 = Hbb::from_cxcywh(
            3.0 * frame_width / 4.0,
            3.0 * frame_height / 4.0,
            100.0,
            100.0,
        )
        .with_confidence(0.9);
        let hbbs = vec![&head1, &head2, &head3];
        let crop = calculate_crop_area(true, false, frame_width, frame_height, &hbbs).unwrap();
        assert!(matches!(crop, CropResult::Stacked(_, _)));

        // Test more than five heads
        let head1 = Hbb::from_cxcywh(frame_width / 6.0, frame_height / 6.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head2 = Hbb::from_cxcywh(frame_width / 3.0, frame_height / 3.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head3 = Hbb::from_cxcywh(frame_width / 2.0, frame_height / 2.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head4 = Hbb::from_cxcywh(
            2.0 * frame_width / 3.0,
            2.0 * frame_height / 3.0,
            100.0,
            100.0,
        )
        .with_confidence(0.9);
        let head5 = Hbb::from_cxcywh(
            5.0 * frame_width / 6.0,
            5.0 * frame_height / 6.0,
            100.0,
            100.0,
        )
        .with_confidence(0.9);
        let head6 = Hbb::from_cxcywh(frame_width - 100.0, frame_height - 100.0, 100.0, 100.0)
            .with_confidence(0.9);
        let hbbs = vec![&head1, &head2, &head3, &head4, &head5, &head6];
        let crop = calculate_crop_area(true, false, frame_width, frame_height, &hbbs).unwrap();
        assert!(matches!(crop, CropResult::Single(_)));
    }

    #[test]
    fn test_calculate_crop_area_graphic() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;

        // Test no heads with graphic mode
        let heads: Vec<&Hbb> = vec![];
        let crop = calculate_crop_area(true, true, frame_width, frame_height, &heads).unwrap();
        assert!(matches!(crop, CropResult::Resize(_)));

        // Test single head with graphic mode (should still be Single, not Resize)
        let head = Hbb::from_cxcywh(frame_width / 2.0, frame_height / 2.0, 100.0, 100.0)
            .with_confidence(0.9);
        let hbbs = vec![&head];
        let crop = calculate_crop_area(true, true, frame_width, frame_height, &hbbs).unwrap();
        assert!(matches!(crop, CropResult::Single(_)));

        // Test more than five heads with graphic mode
        let head1 = Hbb::from_cxcywh(frame_width / 6.0, frame_height / 6.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head2 = Hbb::from_cxcywh(frame_width / 3.0, frame_height / 3.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head3 = Hbb::from_cxcywh(frame_width / 2.0, frame_height / 2.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head4 = Hbb::from_cxcywh(
            2.0 * frame_width / 3.0,
            2.0 * frame_height / 3.0,
            100.0,
            100.0,
        )
        .with_confidence(0.9);
        let head5 = Hbb::from_cxcywh(
            5.0 * frame_width / 6.0,
            5.0 * frame_height / 6.0,
            100.0,
            100.0,
        )
        .with_confidence(0.9);
        let head6 = Hbb::from_cxcywh(frame_width - 100.0, frame_height - 100.0, 100.0, 100.0)
            .with_confidence(0.9);
        let hbbs = vec![&head1, &head2, &head3, &head4, &head5, &head6];
        let crop = calculate_crop_area(true, true, frame_width, frame_height, &hbbs).unwrap();
        assert!(matches!(crop, CropResult::Single(_)));
    }

    #[test]
    fn test_is_within_percentage() {
        let frame_width = 1920.0; // Standard HD width for testing
        let threshold = 5.0; // 5% threshold
        let max_diff = frame_width * (threshold / 100.0); // Maximum allowed difference

        // Test identical values
        let crop1 = CropArea::new(100.0, 100.0, 200.0, 200.0);
        let crop2 = CropArea::new(100.0, 100.0, 200.0, 200.0);
        assert!(crop1.is_within_percentage(&crop2, frame_width, threshold));

        // Test small difference (well within threshold)
        let crop1 = CropArea::new(100.0, 100.0, 200.0, 200.0);
        let crop2 = CropArea::new(150.0, 100.0, 250.0, 200.0); // 50px difference, well under max_diff
        assert!(crop1.is_within_percentage(&crop2, frame_width, threshold));

        // Test difference exactly at threshold
        let crop1 = CropArea::new(100.0, 100.0, 200.0, 200.0);
        let crop2 = CropArea::new(100.0 + max_diff, 100.0, 200.0 + max_diff, 200.0);
        assert!(crop1.is_within_percentage(&crop2, frame_width, threshold));

        // Test difference just over threshold
        let crop1 = CropArea::new(100.0, 100.0, 200.0, 200.0);
        let crop2 = CropArea::new(100.0 + max_diff + 1.0, 100.0, 200.0 + max_diff + 1.0, 200.0);
        assert!(!crop1.is_within_percentage(&crop2, frame_width, threshold));

        // Test with zero values
        let crop1 = CropArea::new(0.0, 0.0, 0.0, 0.0);
        let crop2 = CropArea::new(0.0, 0.0, 0.0, 0.0);
        assert!(crop1.is_within_percentage(&crop2, frame_width, threshold));

        // Test with one zero value - should be similar if within threshold
        let crop1 = CropArea::new(0.0, 0.0, 0.0, 0.0);
        let crop2 = CropArea::new(1.0, 0.0, 1.0, 0.0);
        assert!(crop1.is_within_percentage(&crop2, frame_width, threshold)); // Should be similar since difference is within threshold

        // Test with different threshold values
        let crop1 = CropArea::new(100.0, 100.0, 200.0, 200.0);
        let crop2 = CropArea::new(200.0, 100.0, 300.0, 200.0); // 100px difference
        assert!(!crop1.is_within_percentage(&crop2, frame_width, 5.0)); // Over 5%
        assert!(crop1.is_within_percentage(&crop2, frame_width, 10.0)); // Under 10%
        assert!(crop1.is_within_percentage(&crop2, frame_width, 15.0)); // Under 15%

        // Test with mixed differences
        let crop1 = CropArea::new(100.0, 100.0, 200.0, 200.0);
        let crop2 = CropArea::new(150.0, 100.0, 300.0, 200.0); // x within threshold, width over threshold
        assert!(!crop1.is_within_percentage(&crop2, frame_width, threshold));
    }

    #[test]
    fn test_calculate_two_heads_crop_far_no_stack() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;

        // Test far heads with use_stack_crop = false
        let head1 = Hbb::from_cxcywh(frame_width / 4.0, frame_height / 2.0, 100.0, 100.0);
        let head2 = Hbb::from_cxcywh(3.0 * frame_width / 4.0, frame_height / 2.0, 100.0, 100.0);
        let crop = calculate_two_heads_crop(false, frame_width, frame_height, &head1, &head2);

        match crop {
            CropResult::Single(crop) => {
                // Should return a single crop based on the largest head
                // Height should match frame height
                assert!((crop.height - frame_height).abs() < 1.0);

                // Width should be 3/4 of the height (3:4 aspect ratio)
                let expected_width = frame_height * (3.0 / 4.0);
                assert!((crop.width - expected_width).abs() < 1.0);

                // Should be within frame bounds
                assert!(crop.x >= 0.0);
                assert!(crop.y >= 0.0);
                assert!(crop.x + crop.width <= frame_width);
                assert!(crop.y + crop.height <= frame_height);
            }
            _ => panic!("Expected single crop when use_stack_crop is false"),
        }
    }

    #[test]
    fn test_calculate_four_and_five_heads_crop_far_no_stack() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;

        // Create three heads that are far apart horizontally
        let head1 = Hbb::from_cxcywh(200.0, frame_height / 2.0, 100.0, 100.0);
        let head2 = Hbb::from_cxcywh(1000.0, frame_height / 2.0, 200.0, 100.0); // Largest head
        let head3 = Hbb::from_cxcywh(1800.0, frame_height / 2.0, 100.0, 100.0);
        let heads = vec![&head1, &head2, &head3];

        let crop = calculate_four_and_five_heads_crop(false, frame_width, frame_height, &heads);

        match crop {
            CropResult::Single(crop) => {
                // Should return a single crop based on the largest head (head2)
                // Height should match frame height
                assert!((crop.height - frame_height).abs() < 1.0);

                // Width should be 3/4 of the height (3:4 aspect ratio)
                let expected_width = frame_height * (3.0 / 4.0);
                assert!((crop.width - expected_width).abs() < 1.0);

                // Should be centered on the largest head (head2)
                let largest_head_center = head2.cx();
                assert!((crop.x + crop.width / 2.0 - largest_head_center).abs() < 1.0);

                // Should be within frame bounds
                assert!(crop.x >= 0.0);
                assert!(crop.y >= 0.0);
                assert!(crop.x + crop.width <= frame_width);
                assert!(crop.y + crop.height <= frame_height);
            }
            _ => panic!("Expected single crop when use_stack_crop is false"),
        }
    }

    #[test]
    fn test_calculate_three_heads_crop_real_world() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;

        let head1 = Hbb::from_xyxy(114.34512, 265.61224, 304.0035, 513.53564);
        let head2 = Hbb::from_xyxy(531.13, 213.28334, 704.7175, 470.2871);
        let head3 = Hbb::from_xyxy(943.43054, 278.49518, 1161.655, 579.9011);
        let heads = vec![&head1, &head2, &head3];
        let crop = calculate_three_heads_crop(true, frame_width, frame_height, &heads);
        match crop {
            CropResult::Stacked(crop1, crop2) => {
                // First crop should be optimized for two heads (80% height, 9:6 aspect ratio)
                let expected_crop1_height = frame_height * 0.8;
                let expected_crop1_width = expected_crop1_height * 1.5;
                assert!((crop1.height - expected_crop1_height).abs() < 1.0);
                assert!((crop1.width - expected_crop1_width).abs() < 1.0);
                assert!((crop1.y - frame_height * 0.1).abs() < 1.0);

                // Second crop should be optimized for single head (80% height, 9:10 aspect ratio)
                let expected_crop2_height = frame_height * 0.8;
                let expected_crop2_width = expected_crop2_height * 0.9;
                assert!((crop2.height - expected_crop2_height).abs() < 1.0);
                assert!((crop2.width - expected_crop2_width).abs() < 1.0);
                assert!((crop2.y - frame_height * 0.15).abs() < 1.0);

                // Both crops should be within frame bounds
                assert!(crop1.x >= 0.0);
                assert!(crop1.x + crop1.width <= frame_width);
                assert!(crop1.y >= 0.0);
                assert!(crop1.y + crop1.height <= frame_height);
                assert!(crop2.x >= 0.0);
                assert!(crop2.x + crop2.width <= frame_width);
                assert!(crop2.y >= 0.0);
                assert!(crop2.y + crop2.height <= frame_height);

                // First crop should contain the leftmost two heads
                let head1_xmin = head1.xmin();
                let head1_xmax = head1.xmax();
                let head2_xmin = head2.xmin();
                let head2_xmax = head2.xmax();
                let head1_in_crop1 = head1_xmin >= crop1.x && head1_xmax <= crop1.x + crop1.width;
                let head2_in_crop1 = head2_xmin >= crop1.x && head2_xmax <= crop1.x + crop1.width;
                assert!(
                    head1_in_crop1,
                    "First crop should contain the leftmost head"
                );
                assert!(head2_in_crop1, "First crop should contain head2");

                // Second crop should contain the rightmost head
                let head3_xmin = head3.xmin();
                let head3_xmax = head3.xmax();
                let head3_in_crop2 = head3_xmin >= crop2.x && head3_xmax <= crop2.x + crop2.width;
                assert!(head3_in_crop2, "Second crop should contain head3");
            }
            _ => panic!("Expected stacked crops for real world case"),
        }
    }

    #[test]
    fn test_calculate_three_heads_crop_real_world_lex() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;

        let head1 = Hbb::from_xyxy(459.09668, 252.47464, 587.0282, 434.82794);
        let head2 = Hbb::from_xyxy(864.88776, 344.61285, 1026.0613, 568.9608);
        let head3 = Hbb::from_xyxy(1477.2578, 277.67084, 1673.3591, 527.8382);
        let heads = vec![&head1, &head2, &head3];
        let crop = calculate_three_heads_crop(true, frame_width, frame_height, &heads);
        match crop {
            CropResult::Stacked(crop1, crop2) => {
                // First crop should be optimized for two heads (80% height, 9:6 aspect ratio)
                let expected_crop1_height = frame_height * 0.8;
                let expected_crop1_width = expected_crop1_height * 1.5;
                assert!((crop1.height - expected_crop1_height).abs() < 1.0);
                assert!((crop1.width - expected_crop1_width).abs() < 1.0);
                assert!((crop1.y - frame_height * 0.1).abs() < 1.0);

                // Second crop should be optimized for single head (80% height, 9:10 aspect ratio)
                let expected_crop2_height = frame_height * 0.8;
                let expected_crop2_width = expected_crop2_height * 0.9;
                assert!((crop2.height - expected_crop2_height).abs() < 1.0);
                assert!((crop2.width - expected_crop2_width).abs() < 1.0);
                assert!((crop2.y - frame_height * 0.15).abs() < 1.0);

                // Both crops should be within frame bounds
                assert!(crop1.x >= 0.0);
                assert!(crop1.x + crop1.width <= frame_width);
                assert!(crop1.y >= 0.0);
                assert!(crop1.y + crop1.height <= frame_height);
                assert!(crop2.x >= 0.0);
                assert!(crop2.x + crop2.width <= frame_width);
                assert!(crop2.y >= 0.0);
                assert!(crop2.y + crop2.height <= frame_height);

                // First crop should contain the leftmost two heads
                let head1_xmin = head1.xmin();
                let head1_xmax = head1.xmax();
                let head2_xmin = head2.xmin();
                let head2_xmax = head2.xmax();
                let head1_in_crop1 = head1_xmin >= crop1.x && head1_xmax <= crop1.x + crop1.width;
                let head2_in_crop1 = head2_xmin >= crop1.x && head2_xmax <= crop1.x + crop1.width;
                assert!(
                    head1_in_crop1,
                    "First crop should contain the leftmost head"
                );
                assert!(head2_in_crop1, "First crop should contain head2");

                // Second crop should contain the rightmost head
                let head3_xmin = head3.xmin();
                let head3_xmax = head3.xmax();
                let head3_in_crop2 = head3_xmin >= crop2.x && head3_xmax <= crop2.x + crop2.width;
                assert!(head3_in_crop2, "Second crop should contain head3");
            }
            _ => panic!("Expected stacked crops for real world case"),
        }
    }

    #[test]
    fn test_calculate_three_heads_crop_special_case() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;

        // Create three heads that are similar in size and equally spaced
        // All heads have similar area (100x100 = 10000)
        let head1 = Hbb::from_cxcywh(400.0, frame_height / 2.0, 100.0, 100.0);
        let head2 = Hbb::from_cxcywh(960.0, frame_height / 2.0, 100.0, 100.0); // Center
        let head3 = Hbb::from_cxcywh(1520.0, frame_height / 2.0, 100.0, 100.0);
        let heads = vec![&head1, &head2, &head3];

        let crop = calculate_three_heads_crop(true, frame_width, frame_height, &heads);

        match crop {
            CropResult::Stacked(crop1, crop2) => {
                // First crop should be optimized for two heads (80% height, 9:6 aspect ratio)
                let expected_crop1_height = frame_height * 0.8;
                let expected_crop1_width = expected_crop1_height * 1.5;
                assert!((crop1.height - expected_crop1_height).abs() < 1.0);
                assert!((crop1.width - expected_crop1_width).abs() < 1.0);
                assert!((crop1.y - frame_height * 0.1).abs() < 1.0);

                // Second crop should be optimized for single head (80% height, 9:10 aspect ratio)
                let expected_crop2_height = frame_height * 0.8;
                let expected_crop2_width = expected_crop2_height * 0.9;
                assert!((crop2.height - expected_crop2_height).abs() < 1.0);
                assert!((crop2.width - expected_crop2_width).abs() < 1.0);
                assert!((crop2.y - frame_height * 0.15).abs() < 1.0);

                // Both crops should be within frame bounds
                assert!(crop1.x >= 0.0);
                assert!(crop1.x + crop1.width <= frame_width);
                assert!(crop1.y >= 0.0);
                assert!(crop1.y + crop1.height <= frame_height);
                assert!(crop2.x >= 0.0);
                assert!(crop2.x + crop2.width <= frame_width);
                assert!(crop2.y >= 0.0);
                assert!(crop2.y + crop2.height <= frame_height);

                // First crop should contain the leftmost two heads
                let head1_xmin = head1.xmin();
                let head1_xmax = head1.xmax();
                let head2_xmin = head2.xmin();
                let head2_xmax = head2.xmax();
                let head1_in_crop1 = head1_xmin >= crop1.x && head1_xmax <= crop1.x + crop1.width;
                let head2_in_crop1 = head2_xmin >= crop1.x && head2_xmax <= crop1.x + crop1.width;
                assert!(
                    head1_in_crop1,
                    "First crop should contain the leftmost head"
                );
                assert!(head2_in_crop1, "First crop should contain head2");

                // Second crop should contain the rightmost head
                let head3_xmin = head3.xmin();
                let head3_xmax = head3.xmax();
                let head3_in_crop2 = head3_xmin >= crop2.x && head3_xmax <= crop2.x + crop2.width;
                assert!(head3_in_crop2, "Second crop should contain head3");
            }
            _ => panic!("Expected stacked crops for special three heads case"),
        }
    }

    #[test]
    fn test_calculate_three_heads_crop_fallback() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;

        // Create three heads that are NOT similar in size (should trigger fallback)
        let head1 = Hbb::from_cxcywh(400.0, frame_height / 2.0, 100.0, 100.0);
        let head2 = Hbb::from_cxcywh(960.0, frame_height / 2.0, 200.0, 200.0); // Much larger
        let head3 = Hbb::from_cxcywh(1520.0, frame_height / 2.0, 100.0, 100.0);
        let heads = vec![&head1, &head2, &head3];

        let crop = calculate_three_heads_crop(true, frame_width, frame_height, &heads);

        // Should fall back to the four_and_five_heads logic
        // Since heads are far apart, should get stacked crops with default dimensions
        match crop {
            CropResult::Stacked(crop1, crop2) => {
                // Should have stacked crop dimensions mirroring two-heads behavior
                let expected_width = frame_width * 0.5;
                let expected_height = expected_width * (8.0 / 9.0);
                assert!((crop1.height - expected_height).abs() < 1.0);
                assert!((crop2.height - expected_height).abs() < 1.0);
                assert!((crop1.width - expected_width).abs() < 1.0);
                assert!((crop2.width - expected_width).abs() < 1.0);
            }
            _ => panic!("Expected stacked crops for fallback case"),
        }
    }

    #[test]
    fn test_calculate_crop_area_no_stack() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;

        // Test two heads with use_stack_crop = false
        let head1 = Hbb::from_cxcywh(frame_width / 4.0, frame_height / 2.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head2 = Hbb::from_cxcywh(3.0 * frame_width / 4.0, frame_height / 2.0, 100.0, 100.0)
            .with_confidence(0.9);
        let hbbs = vec![&head1, &head2];
        let crop = calculate_crop_area(false, false, frame_width, frame_height, &hbbs).unwrap();
        assert!(matches!(crop, CropResult::Single(_)));

        // Test three heads with use_stack_crop = false
        let head1 = Hbb::from_cxcywh(frame_width / 4.0, frame_height / 4.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head2 = Hbb::from_cxcywh(frame_width / 2.0, frame_height / 2.0, 200.0, 200.0) // Largest head
            .with_confidence(0.9);
        let head3 = Hbb::from_cxcywh(
            3.0 * frame_width / 4.0,
            3.0 * frame_height / 4.0,
            100.0,
            100.0,
        )
        .with_confidence(0.9);
        let hbbs = vec![&head1, &head2, &head3];
        let crop = calculate_crop_area(false, false, frame_width, frame_height, &hbbs).unwrap();
        assert!(matches!(crop, CropResult::Single(_)));
    }

    #[test]
    fn test_has_crop_class_changed() {
        // Test same class - should return true
        assert!(is_crop_class_same(0, 0)); // Both 0 heads
        assert!(is_crop_class_same(1, 1)); // Both 1 head
        assert!(is_crop_class_same(2, 2)); // Both 2 heads
        assert!(is_crop_class_same(3, 3)); // Both 3 heads
        assert!(is_crop_class_same(4, 4)); // Both 4 heads
        assert!(is_crop_class_same(5, 5)); // Both 5 heads
        assert!(is_crop_class_same(6, 6)); // Both 6 heads
        assert!(is_crop_class_same(10, 10)); // Both 10 heads

        // Test different classes - should return false
        assert!(!is_crop_class_same(0, 1)); // 0 heads vs 1 head
        assert!(!is_crop_class_same(1, 2)); // 1 head vs 2 heads
        assert!(!is_crop_class_same(2, 3)); // 2 heads vs 3 heads
        assert!(!is_crop_class_same(3, 4)); // 3 heads vs 4 heads
        assert!(!is_crop_class_same(0, 6)); // 0 heads vs 6 heads
        assert!(!is_crop_class_same(1, 10)); // 1 head vs 10 heads

        // Test within same class - should return true
        assert!(is_crop_class_same(4, 5)); // Both in 4-5 class
        assert!(is_crop_class_same(6, 7)); // Both in 6+ class
        assert!(is_crop_class_same(6, 10)); // Both in 6+ class
        assert!(is_crop_class_same(7, 15)); // Both in 6+ class

        // Test edge cases
        assert!(!is_crop_class_same(2, 3)); // Edge between 2 and 3
        assert!(!is_crop_class_same(3, 4)); // Edge between 3 and 4-5
        assert!(!is_crop_class_same(0, 100)); // Extreme difference
    }

    #[test]
    fn test_is_crop_similar_resize() {
        let frame_width = 1920.0;
        let threshold = 5.0;

        // Test identical resize crops
        let crop1 = CropResult::Resize(CropArea::new(0.0, 0.0, frame_width, 1080.0));
        let crop2 = CropResult::Resize(CropArea::new(0.0, 0.0, frame_width, 1080.0));
        assert!(is_crop_similar(&crop1, &crop2, frame_width, threshold));

        // Test different resize crops (should be similar since they're both full frame)
        let crop1 = CropResult::Resize(CropArea::new(0.0, 0.0, frame_width, 1080.0));
        let crop2 = CropResult::Resize(CropArea::new(0.0, 0.0, frame_width + 10.0, 1080.0));
        assert!(is_crop_similar(&crop1, &crop2, frame_width, threshold));

        // Test resize vs single (should be false)
        let crop1 = CropResult::Resize(CropArea::new(0.0, 0.0, frame_width, 1080.0));
        let crop2 = CropResult::Single(CropArea::new(100.0, 100.0, 200.0, 200.0));
        assert!(!is_crop_similar(&crop1, &crop2, frame_width, threshold));
    }

    #[test]
    fn test_is_crop_similar_real_values() {
        let frame_width = 1920.0;
        let threshold = 10.0;

        // Test with real values
        let crop1 = CropResult::Stacked(
            CropArea::new(32.520447, 60.0, 1080.0, 960.0),
            CropArea::new(790.54004, 60.0, 1080.0, 960.0),
        );
        let crop2 = CropResult::Stacked(
            CropArea::new(0.0, 60.0, 1080.0, 960.0),
            CropArea::new(840.0, 60.0, 1080.0, 960.0),
        );
        assert!(is_crop_similar(&crop1, &crop2, frame_width, threshold));
    }

    #[test]
    fn test_calculate_six_or_more_heads_crop_close() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;

        // Test six heads that are close together (within 3/4 of frame height)
        let head1 = Hbb::from_cxcywh(frame_width / 2.0 - 150.0, frame_height / 2.0, 100.0, 100.0);
        let head2 = Hbb::from_cxcywh(frame_width / 2.0 - 100.0, frame_height / 2.0, 100.0, 100.0);
        let head3 = Hbb::from_cxcywh(frame_width / 2.0 - 50.0, frame_height / 2.0, 100.0, 100.0);
        let head4 = Hbb::from_cxcywh(frame_width / 2.0, frame_height / 2.0, 100.0, 100.0);
        let head5 = Hbb::from_cxcywh(frame_width / 2.0 + 50.0, frame_height / 2.0, 100.0, 100.0);
        let head6 = Hbb::from_cxcywh(frame_width / 2.0 + 100.0, frame_height / 2.0, 100.0, 100.0);
        let heads = vec![&head1, &head2, &head3, &head4, &head5, &head6];

        let crop = calculate_six_or_more_heads_crop(true, frame_width, frame_height, &heads);

        match crop {
            CropResult::Single(crop) => {
                // Height should match frame height
                assert!((crop.height - frame_height).abs() < 1.0);

                // Width should be 3/4 of the height
                let expected_width = frame_height * (3.0 / 4.0);
                assert!((crop.width - expected_width).abs() < 1.0);

                // Calculate the bounding box of all heads
                let bbox = calculate_bounding_box(&heads);
                let bbox_center_x = bbox.x + bbox.width / 2.0;

                // Crop should be centered on the bounding box center
                assert!((crop.x + crop.width / 2.0 - bbox_center_x).abs() < 1.0);

                // Should be within frame bounds
                assert!(crop.x >= 0.0);
                assert!(crop.y >= 0.0);
                assert!(crop.x + crop.width <= frame_width);
                assert!(crop.y + crop.height <= frame_height);

                // Should contain all heads
                assert!(crop.x <= head1.cx());
                assert!(crop.x + crop.width >= head6.cx());
            }
            _ => panic!("Expected single crop for close heads case"),
        }
    }

    #[test]
    fn test_calculate_six_or_more_heads_crop_with_large_head() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;

        // Test six heads with one that's 3 times larger than the others
        let head1 = Hbb::from_cxcywh(frame_width / 4.0, frame_height / 2.0, 100.0, 100.0);
        let head2 = Hbb::from_cxcywh(frame_width / 3.0, frame_height / 2.0, 100.0, 100.0);
        let head3 = Hbb::from_cxcywh(frame_width / 2.0, frame_height / 2.0, 100.0, 100.0);
        let head4 = Hbb::from_cxcywh(2.0 * frame_width / 3.0, frame_height / 2.0, 100.0, 100.0);
        let head5 = Hbb::from_cxcywh(3.0 * frame_width / 4.0, frame_height / 2.0, 100.0, 100.0);
        // Large head that's 3 times bigger than the others (300x300 vs 100x100 = 9x area)
        let head6 = Hbb::from_cxcywh(frame_width / 2.0, frame_height / 2.0, 300.0, 300.0);
        let heads = vec![&head1, &head2, &head3, &head4, &head5, &head6];

        let crop = calculate_six_or_more_heads_crop(true, frame_width, frame_height, &heads);

        match crop {
            CropResult::Stacked(crop1, crop2) => {
                // Both crops should have height of 8/9 of half frame width
                let expected_width = frame_width * 0.5;
                let expected_height = expected_width * (8.0 / 9.0);
                assert!((crop1.height - expected_height).abs() < 1.0);
                assert!((crop2.height - expected_height).abs() < 1.0);

                // Both crops should have width equal to half frame width
                assert!((crop1.width - expected_width).abs() < 1.0);
                assert!((crop2.width - expected_width).abs() < 1.0);

                // Both crops should be centered vertically
                let expected_y = (frame_height - expected_height) / 2.0;
                assert!((crop1.y - expected_y).abs() < 1.0);
                assert!((crop2.y - expected_y).abs() < 1.0);

                // First crop should be centered on the large head (head6)
                let large_head_center = head6.cx();
                assert!((crop1.x + crop1.width / 2.0 - large_head_center).abs() < 1.0);

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
            _ => panic!("Expected stacked crops for large head case"),
        }
    }

    #[test]
    fn test_calculate_six_or_more_heads_crop_with_large_head_no_stack() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;

        // Test six heads with one that's 3 times larger than the others, but use_stack_crop = false
        let head1 = Hbb::from_cxcywh(frame_width / 4.0, frame_height / 2.0, 100.0, 100.0);
        let head2 = Hbb::from_cxcywh(frame_width / 3.0, frame_height / 2.0, 100.0, 100.0);
        let head3 = Hbb::from_cxcywh(frame_width / 2.0, frame_height / 2.0, 100.0, 100.0);
        let head4 = Hbb::from_cxcywh(2.0 * frame_width / 3.0, frame_height / 2.0, 100.0, 100.0);
        let head5 = Hbb::from_cxcywh(3.0 * frame_width / 4.0, frame_height / 2.0, 100.0, 100.0);
        // Large head that's 3 times bigger than the others
        let head6 = Hbb::from_cxcywh(frame_width / 2.0, frame_height / 2.0, 300.0, 300.0);
        let heads = vec![&head1, &head2, &head3, &head4, &head5, &head6];

        let crop = calculate_six_or_more_heads_crop(false, frame_width, frame_height, &heads);

        match crop {
            CropResult::Single(crop) => {
                // Should return a single crop centered on the large head
                // Height should match frame height
                assert!((crop.height - frame_height).abs() < 1.0);

                // Width should be 3/4 of the height (3:4 aspect ratio)
                let expected_width = frame_height * (3.0 / 4.0);
                assert!((crop.width - expected_width).abs() < 1.0);

                // Should be centered on the large head (head6)
                let large_head_center = head6.cx();
                assert!((crop.x + crop.width / 2.0 - large_head_center).abs() < 1.0);

                // Should be within frame bounds
                assert!(crop.x >= 0.0);
                assert!(crop.y >= 0.0);
                assert!(crop.x + crop.width <= frame_width);
                assert!(crop.y + crop.height <= frame_height);
            }
            _ => panic!("Expected single crop when use_stack_crop is false"),
        }
    }

    #[test]
    fn test_calculate_six_or_more_heads_crop_no_large_head() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;

        // Test six heads that are far apart and no head is 3 times larger than others
        // Make sure the bounding box width is greater than 3/4 of frame height
        // All heads must have exactly the same area to avoid one being considered "large"
        let head1 = Hbb::from_xyxy(1204.4794, 259.98706, 1263.6251, 338.74368);
        let head2 = Hbb::from_xyxy(165.4296, 204.68535, 231.6294, 278.2456);
        let head3 = Hbb::from_xyxy(269.37784, 235.31018, 334.4793, 320.63513);
        let head4 = Hbb::from_xyxy(497.31262, 304.38455, 545.0585, 366.63437);
        let head5 = Hbb::from_xyxy(1573.1497, 222.41495, 1644.2009, 311.7543);
        let head6 = Hbb::from_xyxy(1359.9382, 202.51102, 1425.143, 283.78268);
        let head7 = Hbb::from_xyxy(1119.0876, 314.16095, 1164.9473, 367.55884);
        let head8 = Hbb::from_xyxy(1004.2882, 279.04803, 1053.5553, 333.7185);
        let head9 = Hbb::from_xyxy(1682.1179, 164.2292, 1732.0024, 222.09727);
        let head10 = Hbb::from_xyxy(1316.1659, 216.26654, 1361.8198, 279.69714);
        let head11 = Hbb::from_xyxy(747.9746, 290.0324, 799.1831, 354.62122);
        let head12 = Hbb::from_xyxy(64.98474, 272.07635, 135.15686, 358.80164);
        let head13 = Hbb::from_xyxy(548.1885, 238.84857, 596.61804, 293.81744);
        let head14 = Hbb::from_xyxy(404.89105, 273.33435, 455.97382, 324.4703);
        let head15 = Hbb::from_xyxy(640.2843, 268.06158, 691.5074, 330.74475);
        let head16 = Hbb::from_xyxy(792.7516, 244.46857, 846.63715, 314.46362);
        let head17 = Hbb::from_xyxy(1525.2106, 225.71266, 1579.5985, 292.49323);
        let head18 = Hbb::from_xyxy(904.9985, 297.55618, 951.48126, 358.47745);
        let head19 = Hbb::from_xyxy(327.26227, 255.41493, 377.07877, 321.57587);
        let head20 = Hbb::from_xyxy(680.5171, 259.2168, 719.5209, 310.3493);
        let head21 = Hbb::from_xyxy(129.4746, 170.53577, 184.49347, 230.3624);
        let heads = vec![
            &head1, &head2, &head3, &head4, &head5, &head6, &head7, &head8, &head9, &head10,
            &head11, &head12, &head13, &head14, &head15, &head16, &head17, &head18, &head19,
            &head20, &head21,
        ];

        let crop = calculate_six_or_more_heads_crop(true, frame_width, frame_height, &heads);

        match crop {
            CropResult::Single(crop) => {
                // Should return a single crop (from calculate_no_heads_crop)
                // Height should match frame height
                assert!((crop.height - frame_height).abs() < 1.0);

                // Width should be 3/4 of the height (3:4 aspect ratio)
                let expected_width = frame_height * (3.0 / 4.0);
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
            _ => panic!("Expected single crop for no large head case"),
        }
    }

    #[test]
    fn test_calculate_crop_area_six_or_more_heads() {
        let frame_width = 1920.0;
        let frame_height = 1080.0;

        // Test six heads with use_stack_crop = true
        let head1 = Hbb::from_cxcywh(frame_width / 6.0, frame_height / 6.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head2 = Hbb::from_cxcywh(frame_width / 3.0, frame_height / 3.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head3 = Hbb::from_cxcywh(frame_width / 2.0, frame_height / 2.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head4 = Hbb::from_cxcywh(
            2.0 * frame_width / 3.0,
            2.0 * frame_height / 3.0,
            100.0,
            100.0,
        )
        .with_confidence(0.9);
        let head5 = Hbb::from_cxcywh(
            5.0 * frame_width / 6.0,
            5.0 * frame_height / 6.0,
            100.0,
            100.0,
        )
        .with_confidence(0.9);
        let head6 = Hbb::from_cxcywh(frame_width - 100.0, frame_height - 100.0, 100.0, 100.0)
            .with_confidence(0.9);
        let hbbs = vec![&head1, &head2, &head3, &head4, &head5, &head6];
        let crop = calculate_crop_area(true, false, frame_width, frame_height, &hbbs).unwrap();
        assert!(matches!(crop, CropResult::Single(_)));

        // Test six heads with one large head
        let head1 = Hbb::from_cxcywh(frame_width / 4.0, frame_height / 2.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head2 = Hbb::from_cxcywh(frame_width / 3.0, frame_height / 2.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head3 = Hbb::from_cxcywh(frame_width / 2.0, frame_height / 2.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head4 = Hbb::from_cxcywh(2.0 * frame_width / 3.0, frame_height / 2.0, 100.0, 100.0)
            .with_confidence(0.9);
        let head5 = Hbb::from_cxcywh(3.0 * frame_width / 4.0, frame_height / 2.0, 100.0, 100.0)
            .with_confidence(0.9);
        // Large head that's 3 times bigger than the others
        let head6 = Hbb::from_cxcywh(frame_width / 2.0, frame_height / 2.0, 300.0, 300.0)
            .with_confidence(0.9);
        let hbbs = vec![&head1, &head2, &head3, &head4, &head5, &head6];
        let crop = calculate_crop_area(true, false, frame_width, frame_height, &hbbs).unwrap();
        assert!(matches!(crop, CropResult::Stacked(_, _)));
    }
}
