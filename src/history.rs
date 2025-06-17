use usls::Image;
use crate::crop::CropResult;

/// A structure to hold a pair of CropResult and Image
#[derive(Clone)]
pub struct CropImagePair {
    pub crop: CropResult,
    pub image: Image,
}

/// A structure to maintain a history of CropResult and Image pairs
pub struct CropHistory {
    pairs: Vec<CropImagePair>,
}

impl CropHistory {
    /// Create a new empty history
    pub fn new() -> Self {
        Self { pairs: Vec::new() }
    }

    /// Add a new pair to the history
    pub fn add(&mut self, crop: CropResult, image: Image) {
        self.pairs.push(CropImagePair { crop, image });
    }

    /// Remove and return the first pair from the history
    pub fn pop_front(&mut self) -> Option<CropImagePair> {
        if self.pairs.is_empty() {
            None
        } else {
            Some(self.pairs.remove(0))
        }
    }

    /// Get a reference to the first pair without removing it
    pub fn peek_front(&self) -> Option<&CropImagePair> {
        self.pairs.first()
    }

    /// Get the number of pairs in the history
    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    /// Check if the history is empty
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }
} 