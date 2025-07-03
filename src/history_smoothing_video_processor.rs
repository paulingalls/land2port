use crate::cli::Args;
use crate::crop;
use crate::history;
use crate::image;
use crate::video_processor::VideoProcessor;
use crate::video_processor_utils;
use anyhow::Result;
use usls::Viewer;

/// Video processor that handles cropping with history smoothing
pub struct HistorySmoothingVideoProcessor {
    previous_crop: Option<crop::CropResult>,
    previous_object_count: usize,
    history: history::CropHistory,
}

impl HistorySmoothingVideoProcessor {
    /// Creates a new video processor
    pub fn new() -> Self {
        Self {
            previous_crop: None,
            previous_object_count: 0,
            history: history::CropHistory::new(),
        }
    }
}

impl VideoProcessor for HistorySmoothingVideoProcessor {
    /// Processes a single frame with smoothing logic
    fn process_frame_with_smoothing(
        &mut self,
        img: &usls::Image,
        latest_crop: &crop::CropResult,
        objects: &[&usls::Hbb],
        args: &Args,
        viewer: &mut Viewer,
    ) -> Result<()> {
        let current_object_count = objects.len();
        // Compare with previous crop if it exists
        let mut object_count = current_object_count;
        let crop_result: Option<crop::CropResult> = if let Some(prev_crop) = &self.previous_crop {
            let is_same_class =
                crop::is_crop_class_same(current_object_count, self.previous_object_count);
            let is_latest_crop_similar = crop::is_crop_similar(
                latest_crop,
                prev_crop,
                img.width() as f32,
                args.smooth_percentage,
            );

            if is_same_class && is_latest_crop_similar {
                video_processor_utils::debug_println(format_args!(
                    "is_same_class && is_latest_crop_similar"
                ));
                if !self.history.is_empty() {
                    while let Some(frame) = self.history.pop_front() {
                        video_processor_utils::process_and_display_crop(
                            &frame.image,
                            prev_crop,
                            viewer,
                            args.headless,
                        )?;
                    }
                }
                object_count = self.previous_object_count;
                Some(prev_crop.clone())
            } else {
                // Handle crop change without borrowing self mutably
                let mut crop_result: Option<crop::CropResult> = None;

                if self.history.is_empty() {
                    self.history
                        .add(latest_crop.clone(), img.clone(), current_object_count);
                } else {
                    let change_crop = self.history.peek_front().unwrap().crop.clone();
                    let change_object_count = self.history.peek_front().unwrap().object_count;

                    video_processor_utils::debug_println(format_args!(
                        "change_crop: {:?}",
                        change_crop
                    ));
                    video_processor_utils::debug_println(format_args!(
                        "change_object_count: {:?}",
                        change_object_count
                    ));

                    let is_change_crop_similar = crop::is_crop_similar(
                        latest_crop,
                        &change_crop,
                        img.width() as f32,
                        args.smooth_percentage,
                    );
                    let is_change_object_count_similar =
                        crop::is_crop_class_same(current_object_count, change_object_count);

                    video_processor_utils::debug_println(format_args!(
                        "is_change_crop_similar: {:?}",
                        is_change_crop_similar
                    ));
                    video_processor_utils::debug_println(format_args!(
                        "is_change_object_count_similar: {:?}",
                        is_change_object_count_similar
                    ));

                    if is_change_crop_similar && is_change_object_count_similar {
                        if self.history.len() == args.smooth_duration {
                            while let Some(frame) = self.history.pop_front() {
                                video_processor_utils::process_and_display_crop(
                                    &frame.image,
                                    &change_crop,
                                    viewer,
                                    args.headless,
                                )?;
                            }
                            crop_result = Some(change_crop);
                        } else {
                            self.history
                                .add(change_crop.clone(), img.clone(), change_object_count);
                        }
                    } else {
                        let last_frame = self.history.peek_back().unwrap();
                        let is_cut = image::is_cut(&last_frame.image, img)?;
                        let crop_to_use = if is_cut {
                            prev_crop
                        } else {
                            // Choose crop based on whether prev_crop is stacked and change_crop isn't
                            let result = match (prev_crop, &change_crop) {
                                (crop::CropResult::Stacked(_, _), crop::CropResult::Single(_)) => {
                                    &change_crop
                                }
                                _ => prev_crop,
                            };
                            result
                        };
                        while let Some(frame) = self.history.pop_front() {
                            video_processor_utils::process_and_display_crop(
                                &frame.image,
                                crop_to_use,
                                viewer,
                                args.headless,
                            )?;
                        }
                        self.history
                            .add(latest_crop.clone(), img.clone(), current_object_count);
                    }
                }
                crop_result
            }
        } else {
            object_count = current_object_count;
            Some(latest_crop.clone())
        };

        if let Some(crop_result) = crop_result {
            self.previous_crop = Some(crop_result.clone());
            self.previous_object_count = object_count;
            video_processor_utils::process_and_display_crop(
                img,
                &crop_result,
                viewer,
                args.headless,
            )?;
        }
        Ok(())
    }

    /// Override debug info to include history-specific information
    fn print_debug_info(&self, objects: &[&usls::Hbb], latest_crop: &crop::CropResult) {
        let current_object_count = objects.len();
        video_processor_utils::debug_println(format_args!("--------------------------------"));
        video_processor_utils::debug_println(format_args!("objects: {:?}", objects));
        video_processor_utils::debug_println(format_args!("latest_crop: {:?}", latest_crop));
        video_processor_utils::debug_println(format_args!(
            "previous_crop: {:?}",
            self.previous_crop
        ));
        video_processor_utils::debug_println(format_args!(
            "history length: {:?}",
            self.history.len()
        ));
        video_processor_utils::debug_println(format_args!(
            "current_object_count: {}, previous_object_count: {}",
            current_object_count, self.previous_object_count
        ));
    }
}
