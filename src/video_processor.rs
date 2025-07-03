use crate::cli::Args;
use crate::crop;
use crate::video_processor_utils;
use anyhow::Result;
use usls::{Annotator, DataLoader, Viewer, models::YOLO};

/// Base trait for video processors that handle cropping with different smoothing strategies
pub trait VideoProcessor {
    /// Processes a video with cropping and smoothing
    fn process_video(
        &mut self,
        args: &Args,
        model: &mut YOLO,
        viewer: &mut Viewer,
        data_loader: &DataLoader,
        annotator: Annotator,
    ) -> Result<()> {
        // Common video processing logic
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
                let objects = video_processor_utils::extract_objects_above_threshold(
                    y,
                    &args.object,
                    args.object_prob_threshold,
                );
                let latest_crop = crop::calculate_crop_area(
                    args.use_stack_crop,
                    img.width() as f32,
                    img.height() as f32,
                    &objects,
                )?;

                // Print debug information
                self.print_debug_info(&objects, &latest_crop);

                if args.smooth_duration > 0 {
                    self.process_frame_with_smoothing(
                        &img,
                        &latest_crop,
                        &objects,
                        args,
                        viewer,
                    )?;
                } else {
                    video_processor_utils::process_and_display_crop(
                        &img,
                        &latest_crop,
                        viewer,
                        args.headless,
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Processes a single frame with smoothing logic (to be implemented by concrete processors)
    fn process_frame_with_smoothing(
        &mut self,
        img: &usls::Image,
        latest_crop: &crop::CropResult,
        objects: &[&usls::Hbb],
        args: &Args,
        viewer: &mut Viewer,
    ) -> Result<()>;

    /// Prints debug information (can be overridden by concrete processors)
    fn print_debug_info(&self, objects: &[&usls::Hbb], latest_crop: &crop::CropResult) {
        video_processor_utils::debug_println(format_args!("--------------------------------"));
        video_processor_utils::debug_println(format_args!("objects: {:?}", objects));
        video_processor_utils::debug_println(format_args!("latest_crop: {:?}", latest_crop));
    }
} 