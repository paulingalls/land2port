use anyhow::Result;
use chrono::Local;
use std::fs;
use std::path::Path;
use usls::{
    Annotator, DataLoader, SKELETON_COCO_19, SKELETON_COLOR_COCO_19, Style, Viewer, models::YOLO,
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
        .with_fps(data_loader.video_decoder().unwrap().frame_rate() as usize)
        .with_saveout(processed_video.clone());

    let dl =data_loader.build()?;

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
        println!("ys: {:?}", ys);

        for (x, y) in xs.iter().zip(ys.iter()) {
            let img = if !args.headless { annotator.annotate(x, y)? } else { x.clone() };

            // Calculate crop areas based on the detection results
            let latest_crop = crop::calculate_crop_area(
                args.use_stack_crop,
                img.width() as f32,
                img.height() as f32,
                y,
                0.7, // head probability threshold
            )?;

            // Compare with previous crop if it exists
            let crop_result: Option<crop::CropResult> = if let Some(prev_crop) = &previous_crop {
                if is_crop_similar(
                    &latest_crop,
                    prev_crop,
                    img.width() as f32,
                    args.smooth_percentage,
                ) {
                    if !history.is_empty() {
                        while let Some(pair) = history.pop_front() {
                            process_and_display_crop(&pair.image, prev_crop, &mut viewer, args.headless)?;
                        }
                    }
                    Some(prev_crop.clone())
                } else {
                    let mut crop_result: Option<crop::CropResult> = None;
                    if history.is_empty() {
                        history.add(latest_crop.clone(), img.clone());
                    } else {
                        let change_crop = history.peek_front().unwrap().crop.clone();
                        if is_crop_similar(
                            &latest_crop,
                            &change_crop,
                            img.width() as f32,
                            args.smooth_percentage,
                        ) {
                            if history.len() == args.smooth_duration {
                                while let Some(pair) = history.pop_front() {
                                    process_and_display_crop(
                                        &pair.image,
                                        &change_crop,
                                        &mut viewer,
                                        args.headless,
                                    )?;
                                }
                                crop_result = Some(change_crop);
                            } else {
                                history.add(change_crop.clone(), img.clone());
                            }
                        } else {
                            while let Some(pair) = history.pop_front() {
                                process_and_display_crop(&pair.image, prev_crop, &mut viewer, args.headless)?;
                            }
                            crop_result = Some(prev_crop.clone());
                        }
                    }
                    crop_result
                }
            } else {
                Some(latest_crop)
            };

            if let Some(crop_result) = crop_result {
                previous_crop = Some(crop_result.clone());
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
