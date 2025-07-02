use crate::cli::Args;
use crate::crop;
use crate::history;
use crate::image;
use crate::video_processor_utils;
use anyhow::Result;
use usls::{Annotator, DataLoader, Viewer, models::YOLO};

/// Video processor that handles cropping with history smoothing
pub struct HistorySmoothingVideoProcessor {
    previous_crop: Option<crop::CropResult>,
    previous_head_count: usize,
    history: history::CropHistory,
}

impl HistorySmoothingVideoProcessor {
    /// Creates a new video processor
    pub fn new() -> Self {
        Self {
            previous_crop: None,
            previous_head_count: 0,
            history: history::CropHistory::new(),
        }
    }

    /// Processes a video with cropping and history smoothing
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
                let heads = video_processor_utils::extract_objects_above_threshold(
                    y,
                    &args.object,
                    args.object_prob_threshold,
                );
                let current_head_count = heads.len();
                let latest_crop = crop::calculate_crop_area(
                    args.use_stack_crop,
                    img.width() as f32,
                    img.height() as f32,
                    &heads,
                )?;

                video_processor_utils::debug_println(format_args!(
                    "--------------------------------"
                ));
                video_processor_utils::debug_println(format_args!("heads: {:?}", heads));
                video_processor_utils::debug_println(format_args!(
                    "latest_crop: {:?}",
                    latest_crop
                ));
                video_processor_utils::debug_println(format_args!(
                    "previous_crop: {:?}",
                    self.previous_crop
                ));
                video_processor_utils::debug_println(format_args!(
                    "history length: {:?}",
                    self.history.len()
                ));
                video_processor_utils::debug_println(format_args!(
                    "current_head_count: {}, previous_head_count: {}",
                    current_head_count, self.previous_head_count
                ));

                if args.smooth_duration > 0 {
                    self.process_frame_with_smoothing(
                        &img,
                        &latest_crop,
                        current_head_count,
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

    /// Processes a single frame with smoothing logic
    fn process_frame_with_smoothing(
        &mut self,
        img: &usls::Image,
        latest_crop: &crop::CropResult,
        current_head_count: usize,
        args: &Args,
        viewer: &mut Viewer,
    ) -> Result<()> {
        // Compare with previous crop if it exists
        let mut head_count = current_head_count;
        let crop_result: Option<crop::CropResult> = if let Some(prev_crop) = &self.previous_crop {
            let is_same_class =
                crop::is_crop_class_same(current_head_count, self.previous_head_count);
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
                head_count = self.previous_head_count;
                Some(prev_crop.clone())
            } else {
                // Handle crop change without borrowing self mutably
                let mut crop_result: Option<crop::CropResult> = None;

                if self.history.is_empty() {
                    self.history
                        .add(latest_crop.clone(), img.clone(), current_head_count);
                } else {
                    let change_crop = self.history.peek_front().unwrap().crop.clone();
                    let change_head_count = self.history.peek_front().unwrap().head_count;

                    video_processor_utils::debug_println(format_args!(
                        "change_crop: {:?}",
                        change_crop
                    ));
                    video_processor_utils::debug_println(format_args!(
                        "change_head_count: {:?}",
                        change_head_count
                    ));

                    let is_change_crop_similar = crop::is_crop_similar(
                        latest_crop,
                        &change_crop,
                        img.width() as f32,
                        args.smooth_percentage,
                    );
                    let is_change_head_count_similar =
                        crop::is_crop_class_same(current_head_count, change_head_count);

                    video_processor_utils::debug_println(format_args!(
                        "is_change_crop_similar: {:?}",
                        is_change_crop_similar
                    ));
                    video_processor_utils::debug_println(format_args!(
                        "is_change_head_count_similar: {:?}",
                        is_change_head_count_similar
                    ));

                    if is_change_crop_similar && is_change_head_count_similar {
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
                                .add(change_crop.clone(), img.clone(), change_head_count);
                        }
                    } else {
                        let last_frame = self.history.peek_back().unwrap();
                        let is_cut = image::is_cut(&last_frame.image, img)?;
                        let crop_to_use = if is_cut {
                            // crop_result = Some(latest_crop.clone());
                            // head_count = current_head_count;
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
                        // if !is_cut {
                            self.history
                                .add(latest_crop.clone(), img.clone(), current_head_count);
                        // }
                    }
                }
                crop_result
            }
        } else {
            head_count = current_head_count;
            Some(latest_crop.clone())
        };

        if let Some(crop_result) = crop_result {
            self.previous_crop = Some(crop_result.clone());
            self.previous_head_count = head_count;
            video_processor_utils::process_and_display_crop(
                img,
                &crop_result,
                viewer,
                args.headless,
            )?;
        }
        Ok(())
    }
}
