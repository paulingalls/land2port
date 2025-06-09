use crate::crop::CropResult;
use anyhow::Result;
use std::collections::VecDeque;
use usls::Image;

pub struct CropBuffer {
    frames: VecDeque<(Image, CropResult, f32)>,
    max_buffer_size: usize,
    similarity_threshold: f32,
    crop_history: VecDeque<CropResult>,  // Keep track of recent crops
    history_size: usize,  // How many recent crops to keep
}

impl CropBuffer {
    pub fn new(max_buffer_seconds: f32, fps: f32) -> Self {
        Self {
            frames: VecDeque::new(),
            max_buffer_size: (max_buffer_seconds * fps) as usize,
            similarity_threshold: 10.0,
            crop_history: VecDeque::new(),
            history_size: 5,  // Keep last 5 crops
        }
    }

    pub fn add_frame(&mut self, frame: Image, crop: CropResult, timestamp: f32) -> Result<()> {
        // Add to crop history
        self.crop_history.push_back(crop.clone());
        if self.crop_history.len() > self.history_size {
            self.crop_history.pop_front();
        }
        
        self.frames.push_back((frame, crop, timestamp));
        Ok(())
    }

    fn are_crops_similar(&self, crop1: &CropResult, crop2: &CropResult, width: f32) -> bool {
        match (crop1, crop2) {
            (CropResult::Single(c1), CropResult::Single(c2)) => {
                c1.is_within_percentage(c2, width, self.similarity_threshold)
            }
            (CropResult::Stacked(c1_top, c1_bottom), CropResult::Stacked(c2_top, c2_bottom)) => {
                c1_top.is_within_percentage(c2_top, width, self.similarity_threshold)
                    && c1_bottom.is_within_percentage(c2_bottom, width, self.similarity_threshold)
            }
            _ => false,
        }
    }

    fn is_crop_similar_to_history(&self, crop: &CropResult, width: f32) -> bool {
        // Check if the crop is similar to any crop in the history
        self.crop_history.iter().any(|hist_crop| self.are_crops_similar(crop, hist_crop, width))
    }

    pub fn process_buffer(&mut self, current_crop: &CropResult, width: f32) -> Result<Vec<Image>> {
        let mut committed_frames = Vec::new();

        // Get the first crop if available
        let first_crop = if let Some((_, crop, _)) = self.frames.front() {
            crop.clone()
        } else {
            return Ok(committed_frames);
        };

        // Case 1: If we have at least 2 frames and crops are similar
        if self.frames.len() >= 2 && self.are_crops_similar(current_crop, &first_crop, width) {
            println!("committing all frames with the first crop");
            while let Some((frame, _, _)) = self.frames.pop_front() {
                let cropped_frame =
                    crate::image::create_cropped_image(&frame, &first_crop, frame.height() as u32)?;
                committed_frames.push(cropped_frame);
            }
        }
        // Case 2: If buffer is at max size
        else if self.frames.len() >= self.max_buffer_size {
            // If the current crop is similar to any crop in history, use the most recent similar crop
            if self.is_crop_similar_to_history(current_crop, width) {
                if let Some(hist_crop) = self.crop_history.back() {
                    println!("committing all frames with the most recent similar crop from history");
                    while let Some((frame, _, _)) = self.frames.pop_front() {
                        let cropped_frame =
                            crate::image::create_cropped_image(&frame, hist_crop, frame.height() as u32)?;
                        committed_frames.push(cropped_frame);
                    }
                }
            } else {
                // If not similar to any history, commit with original crops
                println!("committing all frames with their original crops");
                while let Some((frame, crop, _)) = self.frames.pop_front() {
                    let cropped_frame =
                        crate::image::create_cropped_image(&frame, &crop, frame.height() as u32)?;
                    committed_frames.push(cropped_frame);
                }
            }
        }

        println!("committed_frames: {:?}", committed_frames.len());
        println!("buffer size: {:?}", self.frames.len());

        Ok(committed_frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crop::{CropArea, CropResult};
    use usls::Image;
    use image::RgbImage;

    fn create_test_buffer() -> CropBuffer {
        CropBuffer::new(2.0 / 30.0, 30.0)
    }

    fn create_test_image() -> Image {
        // Create a small test image (10x10 pixels)
        let mut rgb_image = RgbImage::new(10, 10);
        // Fill with a test pattern
        for y in 0..10 {
            for x in 0..10 {
                let pixel = if (x + y) % 2 == 0 {
                    image::Rgb([255, 255, 255]) // White
                } else {
                    image::Rgb([0, 0, 0]) // Black
                };
                rgb_image.put_pixel(x, y, pixel);
            }
        }
        Image::from(rgb_image)
    }

    #[test]
    fn test_single_crops_similar() {
        let buffer = create_test_buffer();
        let width = 1920.0;

        // Test identical crops
        let crop1 = CropResult::Single(CropArea {
            x: 100.0,
            y: 100.0,
            width: 800.0,
            height: 600.0,
        });
        let crop2 = CropResult::Single(CropArea {
            x: 100.0,
            y: 100.0,
            width: 800.0,
            height: 600.0,
        });
        assert!(buffer.are_crops_similar(&crop1, &crop2, width));

        // Test crops within threshold (10% of width = 192 pixels)
        let crop3 = CropResult::Single(CropArea {
            x: 150.0, // 50 pixels difference
            y: 150.0, // 50 pixels difference
            width: 850.0, // 50 pixels difference
            height: 650.0, // 50 pixels difference
        });
        assert!(buffer.are_crops_similar(&crop1, &crop3, width));

        // Test crops outside threshold
        let crop4 = CropResult::Single(CropArea {
            x: 300.0, // 200 pixels difference
            y: 300.0, // 200 pixels difference
            width: 1000.0, // 200 pixels difference
            height: 800.0, // 200 pixels difference
        });
        assert!(!buffer.are_crops_similar(&crop1, &crop4, width));
    }

    #[test]
    fn test_stacked_crops_similar() {
        let buffer = create_test_buffer();
        let width = 1920.0;

        // Test identical stacked crops
        let crop1 = CropResult::Stacked(
            CropArea {
                x: 100.0,
                y: 100.0,
                width: 800.0,
                height: 300.0,
            },
            CropArea {
                x: 100.0,
                y: 400.0,
                width: 800.0,
                height: 300.0,
            },
        );
        let crop2 = CropResult::Stacked(
            CropArea {
                x: 100.0,
                y: 100.0,
                width: 800.0,
                height: 300.0,
            },
            CropArea {
                x: 100.0,
                y: 400.0,
                width: 800.0,
                height: 300.0,
            },
        );
        assert!(buffer.are_crops_similar(&crop1, &crop2, width));

        // Test stacked crops within threshold
        let crop3 = CropResult::Stacked(
            CropArea {
                x: 150.0, // 50 pixels difference
                y: 150.0, // 50 pixels difference
                width: 850.0, // 50 pixels difference
                height: 350.0, // 50 pixels difference
            },
            CropArea {
                x: 150.0, // 50 pixels difference
                y: 450.0, // 50 pixels difference
                width: 850.0, // 50 pixels difference
                height: 350.0, // 50 pixels difference
            },
        );
        assert!(buffer.are_crops_similar(&crop1, &crop3, width));

        // Test stacked crops outside threshold
        let crop4 = CropResult::Stacked(
            CropArea {
                x: 300.0, // 200 pixels difference
                y: 300.0, // 200 pixels difference
                width: 1000.0, // 200 pixels difference
                height: 500.0, // 200 pixels difference
            },
            CropArea {
                x: 300.0, // 200 pixels difference
                y: 600.0, // 200 pixels difference
                width: 1000.0, // 200 pixels difference
                height: 500.0, // 200 pixels difference
            },
        );
        assert!(!buffer.are_crops_similar(&crop1, &crop4, width));
    }

    #[test]
    fn test_different_crop_types() {
        let buffer = create_test_buffer();
        let width = 1920.0;

        let single_crop = CropResult::Single(CropArea {
            x: 100.0,
            y: 100.0,
            width: 800.0,
            height: 600.0,
        });
        let stacked_crop = CropResult::Stacked(
            CropArea {
                x: 100.0,
                y: 100.0,
                width: 800.0,
                height: 300.0,
            },
            CropArea {
                x: 100.0,
                y: 400.0,
                width: 800.0,
                height: 300.0,
            },
        );

        // Different crop types should never be similar
        assert!(!buffer.are_crops_similar(&single_crop, &stacked_crop, width));
    }

    #[test]
    fn test_process_buffer_empty() {
        let mut buffer = create_test_buffer();
        let current_crop = CropResult::Single(CropArea {
            x: 100.0,
            y: 100.0,
            width: 800.0,
            height: 600.0,
        });

        let committed_frames = buffer.process_buffer(&current_crop, 1920.0).unwrap();
        assert_eq!(committed_frames.len(), 0);
    }

    #[test]
    fn test_process_buffer_similar_crops() {
        let mut buffer = create_test_buffer();
        let width = 1920.0;

        // Add frames with similar crops
        let crop1 = CropResult::Single(CropArea {
            x: 100.0,
            y: 100.0,
            width: 800.0,
            height: 600.0,
        });
        let crop2 = CropResult::Single(CropArea {
            x: 150.0, // 50px difference, within threshold
            y: 150.0,
            width: 850.0,
            height: 650.0,
        });

        // Add 3 frames with crop1
        for i in 0..3 {
            buffer.add_frame(create_test_image(), crop1.clone(), i as f32).unwrap();
        }

        // Process buffer with similar crop
        let committed_frames = buffer.process_buffer(&crop2, width).unwrap();
        assert_eq!(committed_frames.len(), 3);
        assert_eq!(buffer.frames.len(), 0); // Buffer should be empty after processing
    }

    #[test]
    fn test_process_buffer_max_size() {
        let mut buffer = create_test_buffer();
        let width = 1920.0;

        // Add frames with different crops
        let crop1 = CropResult::Single(CropArea {
            x: 100.0,
            y: 100.0,
            width: 800.0,
            height: 600.0,
        });
        let crop2 = CropResult::Single(CropArea {
            x: 300.0, // 200px difference, outside threshold
            y: 300.0,
            width: 1000.0,
            height: 800.0,
        });

        // Fill buffer to max size
        for i in 0..buffer.max_buffer_size {
            buffer.add_frame(create_test_image(), crop1.clone(), i as f32).unwrap();
        }

        // Process buffer with different crop
        let committed_frames = buffer.process_buffer(&crop2, width).unwrap();
        assert_eq!(committed_frames.len(), buffer.max_buffer_size);
        assert_eq!(buffer.frames.len(), 0); // Buffer should be empty after processing
    }

    #[test]
    fn test_process_buffer_stacked_crops() {
        let mut buffer = create_test_buffer();
        let width = 1920.0;

        // Create stacked crops
        let crop1 = CropResult::Stacked(
            CropArea {
                x: 100.0,
                y: 100.0,
                width: 800.0,
                height: 300.0,
            },
            CropArea {
                x: 100.0,
                y: 400.0,
                width: 800.0,
                height: 300.0,
            },
        );
        let crop2 = CropResult::Stacked(
            CropArea {
                x: 150.0, // 50px difference, within threshold
                y: 150.0,
                width: 850.0,
                height: 350.0,
            },
            CropArea {
                x: 150.0,
                y: 450.0,
                width: 850.0,
                height: 350.0,
            },
        );

        // Add 3 frames with crop1
        for i in 0..3 {
            buffer.add_frame(create_test_image(), crop1.clone(), i as f32).unwrap();
        }

        // Process buffer with similar stacked crop
        let committed_frames = buffer.process_buffer(&crop2, width).unwrap();
        assert_eq!(committed_frames.len(), 3);
        assert_eq!(buffer.frames.len(), 0); // Buffer should be empty after processing
    }

    #[test]
    fn test_process_buffer_no_commit() {
        let mut buffer = create_test_buffer();
        let width = 1920.0;

        // Add frames with different crops
        let crop1 = CropResult::Single(CropArea {
            x: 100.0,
            y: 100.0,
            width: 800.0,
            height: 600.0,
        });
        let crop2 = CropResult::Single(CropArea {
            x: 300.0, // 200px difference, outside threshold
            y: 300.0,
            width: 1000.0,
            height: 800.0,
        });

        // Add 2 frames (buffer size is 2)
        for i in 0..2 {
            buffer.add_frame(create_test_image(), crop1.clone(), i as f32).unwrap();
        }

        // Process buffer with different crop
        let committed_frames = buffer.process_buffer(&crop2, width).unwrap();
        assert_eq!(committed_frames.len(), 2); // All frames should be committed since buffer is full
        assert_eq!(buffer.frames.len(), 0); // Buffer should be empty after processing
    }

    #[test]
    fn test_alternating_calls_similar_crops() {
        let mut buffer = create_test_buffer();
        let width = 1920.0;

        // Create similar crops (within threshold)
        let crop1 = CropResult::Single(CropArea {
            x: 100.0,
            y: 100.0,
            width: 800.0,
            height: 600.0,
        });
        let crop2 = CropResult::Single(CropArea {
            x: 150.0, // 50px difference, within threshold
            y: 150.0,
            width: 850.0,
            height: 650.0,
        });

        // First frame
        buffer.add_frame(create_test_image(), crop1.clone(), 0.0).unwrap();
        let committed = buffer.process_buffer(&crop1, width).unwrap();
        assert_eq!(committed.len(), 0); // No frames committed yet
        assert_eq!(buffer.frames.len(), 1); // One frame in buffer

        // Second frame with similar crop
        buffer.add_frame(create_test_image(), crop2.clone(), 1.0).unwrap();
        let committed = buffer.process_buffer(&crop2, width).unwrap();
        assert_eq!(committed.len(), 2); // Both frames committed
        assert_eq!(buffer.frames.len(), 0); // Buffer empty
    }

    #[test]
    fn test_alternating_calls_different_crops() {
        let mut buffer = create_test_buffer();
        let width = 1920.0;

        // Create different crops (outside threshold)
        let crop1 = CropResult::Single(CropArea {
            x: 100.0,
            y: 100.0,
            width: 800.0,
            height: 600.0,
        });
        let crop2 = CropResult::Single(CropArea {
            x: 300.0, // 200px difference, outside threshold
            y: 300.0,
            width: 1000.0,
            height: 800.0,
        });

        // First frame
        buffer.add_frame(create_test_image(), crop1.clone(), 0.0).unwrap();
        let committed = buffer.process_buffer(&crop1, width).unwrap();
        assert_eq!(committed.len(), 0); // No frames committed yet
        assert_eq!(buffer.frames.len(), 1); // One frame in buffer

        // Second frame with different crop (buffer should be full after this add)
        buffer.add_frame(create_test_image(), crop2.clone(), 1.0).unwrap();
        let committed = buffer.process_buffer(&crop2, width).unwrap();
        assert_eq!(committed.len(), 2); // Both frames committed
        assert_eq!(buffer.frames.len(), 0); // Buffer empty
    }

    #[test]
    fn test_alternating_calls_mixed_crop_types() {
        let mut buffer = create_test_buffer();
        let width = 1920.0;

        // Create single and stacked crops
        let single_crop = CropResult::Single(CropArea {
            x: 100.0,
            y: 100.0,
            width: 800.0,
            height: 600.0,
        });
        let stacked_crop = CropResult::Stacked(
            CropArea {
                x: 100.0,
                y: 100.0,
                width: 800.0,
                height: 300.0,
            },
            CropArea {
                x: 100.0,
                y: 400.0,
                width: 800.0,
                height: 300.0,
            },
        );

        // First frame with single crop
        buffer.add_frame(create_test_image(), single_crop.clone(), 0.0).unwrap();
        let committed = buffer.process_buffer(&single_crop, width).unwrap();
        assert_eq!(committed.len(), 0); // No frames committed yet
        assert_eq!(buffer.frames.len(), 1); // One frame in buffer

        // Second frame with stacked crop (buffer should be full after this add)
        buffer.add_frame(create_test_image(), stacked_crop.clone(), 1.0).unwrap();
        let committed = buffer.process_buffer(&stacked_crop, width).unwrap();
        assert_eq!(committed.len(), 2); // Both frames committed
        assert_eq!(buffer.frames.len(), 0); // Buffer empty
    }
}
