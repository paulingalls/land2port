use anyhow::Result;
use chrono::Local;
use std::fs;
use std::path::Path;
use usls::{Annotator, DataLoader, Style, Viewer, models::YOLO};

mod audio;
mod cli;
mod config;
mod crop;
mod history;
mod image;
mod transcript;
mod history_smoothing_video_processor;
mod simple_smoothing_video_processor;
mod video_processor_utils;

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

    // build model
    let mut model = YOLO::new(config.commit()?)?;

    // build dataloader
    let data_loader = DataLoader::new(&args.source)?
        .with_batch(model.batch() as _)
        .build()?;

    let mut viewer = Viewer::default()
        .with_window_scale(0.5)
        .with_fps(data_loader.frame_rate() as usize)
        .with_saveout(processed_video.clone());

    // build annotator
    let annotator = Annotator::default()
        .with_obb_style(Style::obb().with_draw_fill(true))
        .with_hbb_style(
            Style::hbb()
                .with_draw_fill(true)
                .with_palette(&usls::Color::palette_coco_80()),
        );

    if args.use_simple_smoothing {
        let mut processor = simple_smoothing_video_processor::SimpleSmoothingVideoProcessor::new();
        processor.process_video(&args, &mut model, &mut viewer, &data_loader, annotator)?;
    } else {
        let mut processor = history_smoothing_video_processor::HistorySmoothingVideoProcessor::new();
        processor.process_video(&args, &mut model, &mut viewer, &data_loader, annotator)?;
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
