use anyhow::Result;
use chrono::Local;
use std::fs;
use std::path::Path;
use usls::{
    Annotator, DataLoader, Hbb, SKELETON_COCO_19, SKELETON_COLOR_COCO_19, Style, Viewer, Y,
    models::YOLO,
};

mod audio;
mod cli;
mod config;
mod crop;
mod history;
mod image;
mod transcript;

/// Creates a timestamped output directory and returns its path
fn create_output_dir() -> Result<String> {
    let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let output_dir = format!("./runs/{}", timestamp);
    fs::create_dir_all(&output_dir)?;
    Ok(output_dir)
}

fn is_crop_similar(
    crop1: &crop::CropResult,
    crop2: &crop::CropResult,
    width: f32,
    threshold: f32,
) -> bool {
    match (crop1, crop2) {
        (crop::CropResult::Single(crop1), crop::CropResult::Single(crop2)) => {
            crop1.is_within_percentage(crop2, width, threshold)
        }
        (
            crop::CropResult::Stacked(crop1_1, crop1_2),
            crop::CropResult::Stacked(crop2_1, crop2_2),
        ) => {
            crop1_1.is_within_percentage(crop2_1, width, threshold)
                && crop1_2.is_within_percentage(crop2_2, width, threshold)
        }
        _ => false, // If crop types don't match, use the new crop
    }
}

fn process_and_display_crop(
    img: &usls::Image,
    crop_result: &crop::CropResult,
    viewer: &mut Viewer,
    headless: bool,
) -> Result<()> {
    let cropped_img = image::create_cropped_image(img, crop_result, img.height() as u32)?;
    if !headless {
        viewer.imshow(&cropped_img)?;
    }
    viewer.write_video_frame(&cropped_img)?;
    Ok(())
}

/// Extracts head detections above the probability threshold from YOLO detection results
fn extract_heads_above_threshold(detection: &Y, head_prob_threshold: f32) -> Vec<&Hbb> {
    if let Some(hbbs) = detection.hbbs() {
        hbbs.iter()
            .filter(|hbb| {
                if let Some(confidence) = hbb.confidence() {
                    confidence >= head_prob_threshold
                } else {
                    false
                }
            })
            .collect()
    } else {
        vec![]
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: cli::Args = argh::from_env();
    let config = config::build_config(&args)?;

    // Create timestamped output directory
    let output_dir = create_output_dir()?;
    println!("Created output directory: {}", output_dir);

    // Verify ffmpeg is installed
    audio::check_ffmpeg_installed()?;

    // Define output paths
    let extracted_audio = format!("{}/extracted_audio.mp4", output_dir);
    let compressed_audio = format!("{}/compressed_audio.mp3", output_dir);
    let srt_path = format!("{}/transcript.srt", output_dir);
    let processed_video = format!("{}/processed_video.mp4", output_dir);
    let captioned_video = format!("{}/captioned_video.mp4", output_dir);
    let final_video = format!("{}/final_output.mp4", output_dir);

    // Extract audio from the source video
    audio::extract_audio(&args.source, &extracted_audio)?;
    println!("Audio extracted successfully to: {}", extracted_audio);

    // Compress the extracted audio to MP3
    audio::compress_to_mp3(&extracted_audio, &compressed_audio)?;
    println!("Audio compressed to MP3: {}", compressed_audio);

    // Transcribe audio
    println!("Transcribing audio to: {}", srt_path);
    let transcript_config = transcript::TranscriptConfig::default();
    transcript::transcribe_audio(
        Path::new(&compressed_audio),
        Path::new(&srt_path),
        &transcript_config,
    )
    .await?;
    println!("Transcription completed successfully");

    // build model
    let mut model = YOLO::new(config.commit()?)?;

    // build dataloader
    let data_loader = DataLoader::new(&args.source)?.with_batch(model.batch() as _);

    let mut viewer = Viewer::default()
        .with_window_scale(0.5)
        .with_fps(data_loader.frame_rate() as usize)
        .with_saveout(processed_video.clone());

    let dl = data_loader.build()?;

    // build annotator
    let annotator = Annotator::default()
        .with_obb_style(Style::obb().with_draw_fill(true))
        .with_hbb_style(
            Style::hbb()
                .with_draw_fill(true)
                .with_palette(&usls::Color::palette_coco_80()),
        )
        .with_keypoint_style(
            Style::keypoint()
                .with_skeleton((SKELETON_COCO_19, SKELETON_COLOR_COCO_19).into())
                .show_confidence(false)
                .show_id(true)
                .show_name(false),
        )
        .with_mask_style(Style::mask().with_draw_mask_polygon_largest(true));

    // Store the previous crop result
    let mut previous_crop: Option<crop::CropResult> = None;
    let mut previous_head_count: usize = 0;
    let mut history = history::CropHistory::new();

    // run & annotate
    for xs in &dl {
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
        // println!("ys: {:?}", ys);

        for (x, y) in xs.iter().zip(ys.iter()) {
            let img = if !args.headless {
                annotator.annotate(x, y)?
            } else {
                x.clone()
            };

            // Calculate crop areas based on the detection results
            let heads = extract_heads_above_threshold(y, 0.7); // head probability threshold
            let current_head_count = heads.len();
            let latest_crop = crop::calculate_crop_area(
                args.use_stack_crop,
                img.width() as f32,
                img.height() as f32,
                &heads,
            )?;

            println!("--------------------------------");
            println!("heads: {:?}", heads);
            println!("latest_crop: {:?}", latest_crop);
            println!("previous_crop: {:?}", previous_crop);
            println!("history length: {:?}", history.len());
            println!(
                "current_head_count: {}, previous_head_count: {}",
                current_head_count, previous_head_count
            );

            // Compare with previous crop if it exists
            let mut head_count = current_head_count;
            let crop_result: Option<crop::CropResult> = if let Some(prev_crop) = &previous_crop {
                let is_same_class =
                    crop::is_crop_class_same(current_head_count, previous_head_count);
                let is_latest_crop_similar = is_crop_similar(
                    &latest_crop,
                    prev_crop,
                    img.width() as f32,
                    args.smooth_percentage,
                );

                if is_same_class && is_latest_crop_similar {
                    println!("is_same_class && is_latest_crop_similar");
                    if !history.is_empty() {
                        while let Some(frame) = history.pop_front() {
                            process_and_display_crop(
                                &frame.image,
                                &prev_crop,
                                &mut viewer,
                                args.headless,
                            )?;
                        }
                    }
                    head_count = previous_head_count;
                    Some(prev_crop.clone())
                } else {
                    let mut crop_result: Option<crop::CropResult> = None;
                    if history.is_empty() {
                        history.add(latest_crop.clone(), img.clone(), current_head_count);
                    } else {
                        let change_crop = history.peek_front().unwrap().crop.clone();
                        let change_head_count = history.peek_front().unwrap().head_count;
                        println!("change_crop: {:?}", change_crop);
                        println!("change_head_count: {:?}", change_head_count);
                        let is_change_crop_similar = is_crop_similar(
                            &latest_crop,
                            &change_crop,
                            img.width() as f32,
                            args.smooth_percentage,
                        );
                        let is_change_head_count_similar =
                            crop::is_crop_class_same(current_head_count, change_head_count);
                        println!("is_change_crop_similar: {:?}", is_change_crop_similar);
                        println!("is_change_head_count_similar: {:?}", is_change_head_count_similar);

                        if is_change_crop_similar && is_change_head_count_similar {
                            if history.len() == args.smooth_duration {
                                while let Some(frame) = history.pop_front() {
                                    process_and_display_crop(
                                        &frame.image,
                                        &change_crop,
                                        &mut viewer,
                                        args.headless,
                                    )?;
                                }
                                head_count = change_head_count;
                                crop_result = Some(change_crop);
                            } else {
                                history.add(change_crop.clone(), img.clone(), change_head_count);
                            }
                        } else {
                            // Choose crop based on whether prev_crop is stacked and change_crop isn't
                            let crop_to_use = match (&prev_crop, &change_crop) {
                                (crop::CropResult::Stacked(_, _), crop::CropResult::Single(_)) => &change_crop,
                                _ => &prev_crop,
                            };
                            
                            while let Some(frame) = history.pop_front() {
                                process_and_display_crop(
                                    &frame.image,
                                    crop_to_use,
                                    &mut viewer,
                                    args.headless,
                                )?;
                            }
                            history.add(latest_crop.clone(), img.clone(), current_head_count);
                        }
                    }
                    crop_result
                }
            } else {
                head_count = current_head_count;
                Some(latest_crop)
            };

            if let Some(crop_result) = crop_result {
                previous_crop = Some(crop_result.clone());
                previous_head_count = head_count;
                process_and_display_crop(&img, &crop_result, &mut viewer, args.headless)?;
            }
        }
    }

    viewer.finalize_video()?;

    // Burn captions into the video
    println!("Burning captions into video...");
    let caption_style = audio::CaptionStyle::default();
    audio::burn_captions(
        &processed_video,
        &srt_path,
        &captioned_video,
        Some(caption_style),
    )?;
    println!("Captions burned successfully");

    // Add audio to the final video
    println!("Adding audio to video...");
    audio::combine_video_audio(&captioned_video, &extracted_audio, &final_video)?;
    println!(
        "Audio added successfully. Final video saved to: {}",
        final_video
    );

    // summary
    model.summary();

    Ok(())
}
