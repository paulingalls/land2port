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
mod image;
mod transcript;

/// Creates a timestamped output directory and returns its path
fn create_output_dir() -> Result<String> {
    let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let output_dir = format!("./runs/{}", timestamp);
    fs::create_dir_all(&output_dir)?;
    Ok(output_dir)
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

    let mut viewer = Viewer::default()
        .with_window_scale(0.5)
        .with_fps(30)
        .with_saveout(processed_video.clone());

    // build model
    let mut model = YOLO::new(config.commit()?)?;

    // build dataloader
    let dl = DataLoader::new(&args.source)?
        .with_batch(model.batch() as _)
        .build()?;

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
            let img = annotator.annotate(x, y)?;

            // Calculate crop areas based on the detection results
            let new_crop = crop::calculate_crop_area(
                img.width() as f32,
                img.height() as f32,
                y,
                0.5, // head probability threshold
            )?;

            // Compare with previous crop if it exists
            let crop_result = if let Some(prev_crop) = &previous_crop {
                let should_use_prev = match (&new_crop, prev_crop) {
                    (crop::CropResult::Single(new), crop::CropResult::Single(prev)) => {
                        new.is_within_percentage(prev, img.width() as f32, 10.0)
                    }
                    (
                        crop::CropResult::Stacked(new1, new2),
                        crop::CropResult::Stacked(prev1, prev2),
                    ) => {
                        new1.is_within_percentage(prev1, img.width() as f32, 10.0)
                            && new2.is_within_percentage(prev2, img.width() as f32, 10.0)
                    }
                    _ => false, // If crop types don't match, use the new crop
                };

                if should_use_prev {
                    prev_crop.clone()
                } else {
                    new_crop.clone()
                }
            } else {
                new_crop.clone()
            };

            previous_crop = Some(crop_result.clone());

            println!("crop_result: {:?}", crop_result);

            // Create the cropped image
            let cropped_img = image::create_cropped_image(&img, &crop_result, img.height() as u32)?;

            // Display the cropped image
            viewer.imshow(&cropped_img)?;
            viewer.write_video_frame(&cropped_img)?;
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
