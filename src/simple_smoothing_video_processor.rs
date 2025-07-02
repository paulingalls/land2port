use crate::cli::Args;
use crate::crop;
use crate::video_processor_utils;
use anyhow::Result;
use usls::{Annotator, DataLoader, Viewer, models::YOLO};

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

    /// Processes a video with cropping and simple smoothing
    pub fn process_video(
        &mut self,
        args: &Args,
        model: &mut YOLO,
        viewer: &mut Viewer,
        data_loader: &DataLoader,
        annotator: Annotator,
    ) -> Result<()> {
        // run & annotate
        for xs in data_loader {
            if viewer.is_window_exist() && !viewer.is_window_open() {
                break;
            }

            // Handle key events and delay
            if let Some(key) = viewer.wait_key(1) {
                if key == usls::Key::Escape {
                    break;
                }
            }

            let ys = model.forward(&xs)?;

            for (x, y) in xs.iter().zip(ys.iter()) {
                let img = if !args.headless {
                    annotator.annotate(x, y)?
                } else {
                    x.clone()
                };

                // Calculate crop areas based on the detection results
                let heads =
                    video_processor_utils::extract_objects_above_threshold(y, &args.object, args.object_prob_threshold);
                let latest_crop = crop::calculate_crop_area(
                    args.use_stack_crop,
                    img.width() as f32,
                    img.height() as f32,
                    &heads,
                )?;

                video_processor_utils::debug_println(format_args!("--------------------------------"));
                video_processor_utils::debug_println(format_args!("heads: {:?}", heads));
                video_processor_utils::debug_println(format_args!("latest_crop: {:?}", latest_crop));
                video_processor_utils::debug_println(format_args!("previous_crop: {:?}", self.previous_crop));

                if args.smooth_duration > 0 {
                    self.process_frame_with_smoothing(
                        &img,
                        &latest_crop,
                        args,
                        viewer,
                    )?;
                } else {
                    video_processor_utils::process_and_display_crop(&img, &latest_crop, viewer, args.headless)?;
                }
            }
        }
        Ok(())
    }

    /// Processes a single frame with simple smoothing logic
    fn process_frame_with_smoothing(
        &mut self,
        img: &usls::Image,
        latest_crop: &crop::CropResult,
        args: &Args,
        viewer: &mut Viewer,
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
                video_processor_utils::debug_println(format_args!("Using previous crop (same class and similar)"));
                prev_crop.clone()
            } else {
                video_processor_utils::debug_println(format_args!("Using latest crop (different class or not similar)"));
                latest_crop.clone()
            }
        } else {
            video_processor_utils::debug_println(format_args!("No previous crop, using latest crop"));
            latest_crop.clone()
        };

        // Update previous crop and head count
        self.previous_crop = Some(crop_result.clone());

        // Process and display the chosen crop
        video_processor_utils::process_and_display_crop(img, &crop_result, viewer, args.headless)?;
        Ok(())
    }
}

