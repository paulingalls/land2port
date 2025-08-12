use crate::cli::Args;
use crate::crop;
use crate::video_processor_utils;
use crate::video_processor::VideoProcessor;
use anyhow::Result;
use usls::Viewer;

/// Video processor that handles cropping with simple smoothing (no history)
pub struct SimpleSmoothingVideoProcessor {
    previous_crop: Option<crop::CropResult>
}

impl SimpleSmoothingVideoProcessor {
    /// Creates a new simple smoothing video processor
    pub fn new() -> Self {
        Self {
            previous_crop: None
        }
    }
}

impl VideoProcessor for SimpleSmoothingVideoProcessor {
    /// Processes a single frame with simple smoothing logic
    fn process_frame_with_smoothing(
        &mut self,
        img: &usls::Image,
        latest_crop: &crop::CropResult,
        _objects: &[&usls::Hbb],
        args: &Args,
        viewer: &mut Viewer,
        _smooth_duration_frames: usize,
    ) -> Result<()> {
        // Compare with previous crop if it exists
        let crop_result = if let Some(prev_crop) = &self.previous_crop {
            let is_latest_crop_similar = crop::is_crop_similar(
                latest_crop,
                prev_crop,
                img.width() as f32,
                args.smooth_percentage,
            );

            if is_latest_crop_similar {
                video_processor_utils::debug_println(format_args!("Using previous crop (similar)"));
                prev_crop.clone()
            } else {
                video_processor_utils::debug_println(format_args!("Using latest crop (not similar)"));
                latest_crop.clone()
            }
        } else {
            video_processor_utils::debug_println(format_args!("No previous crop, using latest crop"));
            latest_crop.clone()
        };

        self.previous_crop = Some(crop_result.clone());

        // Process and display the chosen crop
        video_processor_utils::process_and_display_crop(img, &crop_result, viewer, args.headless)?;
        Ok(())
    }

    /// Override debug info to include previous crop information
    fn print_debug_info(&self, objects: &[&usls::Hbb], latest_crop: &crop::CropResult, is_graphic: bool) {
        video_processor_utils::print_default_debug_info(objects, latest_crop, is_graphic);
        video_processor_utils::debug_println(format_args!("previous_crop: {:?}", self.previous_crop));
    }
}

