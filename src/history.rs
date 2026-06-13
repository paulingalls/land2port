use crate::crop::CropResult;
use std::collections::VecDeque;
use usls::Image;

/// A structure to hold frame data including crop, image, and head count
#[derive(Clone)]
pub struct FrameData {
    pub crop: CropResult,
    pub image: Image,
    pub object_count: usize,
}

/// A structure to maintain a history of frame data
pub struct CropHistory {
    frames: VecDeque<FrameData>,
}

impl CropHistory {
    /// Create a new empty history
    pub fn new() -> Self {
        Self {
            frames: VecDeque::new(),
        }
    }

    /// Add a new frame to the history
    pub fn add(&mut self, crop: CropResult, image: Image, object_count: usize) {
        self.frames.push_back(FrameData {
            crop,
            image,
            object_count,
        });
    }

    /// Remove and return the first frame from the history (O(1))
    pub fn pop_front(&mut self) -> Option<FrameData> {
        self.frames.pop_front()
    }

    /// Get a reference to the first frame without removing it
    pub fn peek_front(&self) -> Option<&FrameData> {
        self.frames.front()
    }

    /// Get the number of frames in the history
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Check if the history is empty
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crop::{CropArea, CropResult};
    use image::RgbImage;

    fn dummy_image() -> Image {
        Image::from(RgbImage::new(2, 2))
    }

    #[test]
    fn test_history_is_fifo() {
        let mut history = CropHistory::new();
        assert!(history.is_empty());

        for i in 0..3 {
            history.add(
                CropResult::Single(CropArea::new(0.0, 0.0, 2.0, 2.0)),
                dummy_image(),
                i,
            );
        }

        assert_eq!(history.len(), 3);
        assert_eq!(history.peek_front().unwrap().object_count, 0);
        assert_eq!(history.pop_front().unwrap().object_count, 0);
        assert_eq!(history.pop_front().unwrap().object_count, 1);
        assert_eq!(history.pop_front().unwrap().object_count, 2);
        assert!(history.pop_front().is_none());
        assert!(history.is_empty());
    }
}
