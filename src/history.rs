use usls::Image;
use crate::crop::CropResult;

/// A structure to hold frame data including crop, image, and head count
#[derive(Clone)]
pub struct FrameData {
    pub crop: CropResult,
    pub image: Image,
    pub object_count: usize,
}

/// A structure to maintain a history of frame data
pub struct CropHistory {
    frames: Vec<FrameData>,
}

impl CropHistory {
    /// Create a new empty history
    pub fn new() -> Self {
        Self { frames: Vec::new() }
    }

    /// Add a new frame to the history
    pub fn add(&mut self, crop: CropResult, image: Image, object_count: usize) {
        self.frames.push(FrameData { crop, image, object_count });
    }

    /// Remove and return the first frame from the history
    pub fn pop_front(&mut self) -> Option<FrameData> {
        if self.frames.is_empty() {
            None
        } else {
            Some(self.frames.remove(0))
        }
    }

    /// Get a reference to the first frame without removing it
    pub fn peek_front(&self) -> Option<&FrameData> {
        self.frames.first()
    }

    pub fn peek_back(&self) -> Option<&FrameData> {
        self.frames.last()
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