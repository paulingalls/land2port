use crate::video_processor::VideoProcessor;
use anyhow::{Context, Result};
use chrono::Local;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Explicitly fsync a file so that GCS FUSE (or any other FUSE filesystem)
/// flushes its write-back cache to the remote store before the process exits.
fn sync_output_file(path: &str) -> Result<()> {
    // Open with write access: Windows FlushFileBuffers (sync_all) is denied
    // on a read-only handle. Does not truncate the file.
    let f = fs::OpenOptions::new()
        .write(true)
        .open(path)
        .with_context(|| format!("Opening output file for fsync: {}", path))?;
    f.sync_all()
        .with_context(|| format!("Fsyncing output file: {}", path))?;
    println!("Output file synced: {}", path);
    Ok(())
}

mod audio;
mod ball_video_processor;
mod cli;
mod config;
mod crop;
mod frame_sink;
mod history;
mod history_smoothing_video_processor;
mod image;
mod metrics;
mod simple_smoothing_video_processor;
mod transcript;
mod video_processor;
mod video_processor_utils;

/// Creates a timestamped output directory and returns its absolute path.
/// Uses LAND2PORT_RUNS_DIR if set (e.g. /app/runs in container), else cwd/runs.
fn create_output_dir() -> Result<String> {
    let timestamp = Local::now().format("%Y%m%d_%H%M%S_%f").to_string();
    let base: PathBuf = match env::var("LAND2PORT_RUNS_DIR") {
        Ok(d) => PathBuf::from(d),
        Err(_) => env::current_dir().context("Getting current working directory")?.join("runs"),
    };
    let output_dir = base.join(timestamp.as_str());
    let output_dir_str = output_dir.to_string_lossy().into_owned();
    fs::create_dir_all(&output_dir).with_context(|| format!("Creating output directory {}", output_dir.display()))?;
    Ok(output_dir_str)
}

/// Copy a file to a destination path, creating parent dirs. Uses io::copy for
/// compatibility with FUSE (e.g. GCS) where fs::copy can fail.
/// Source must be an absolute path so it is resolved regardless of cwd.
fn copy_to_output(source: &str, dest: &str) -> Result<()> {
    let source_path = Path::new(source);
    let dest_path = Path::new(dest);

    if !source_path.exists() {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        anyhow::bail!(
            "Source file does not exist: {}\n  Current working directory: {}\n  (Use absolute paths for output so cwd changes do not break the copy.)",
            source_path.display(),
            cwd.display()
        );
    }
    let meta = fs::metadata(source_path).with_context(|| format!("Stat source file {}", source))?;
    println!(
        "Copying source {} ({}) to {}",
        source_path.display(),
        human_size(meta.len()),
        dest
    );

    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Creating destination directory {}", parent.display()))?;
    }
    let mut src_file = fs::File::open(source_path)
        .with_context(|| format!("Opening source file {}", source))?;
    let mut dest_file = fs::File::create(dest_path)
        .with_context(|| format!("Creating destination file {}", dest))?;
    io::copy(&mut src_file, &mut dest_file)
        .with_context(|| format!("Copying {} to {}", source, dest))?;
    dest_file.sync_all()
        .with_context(|| format!("Fsyncing destination file {}", dest))?;
    Ok(())
}

fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} GiB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MiB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KiB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    metrics::init();
    let mut args: cli::Args = argh::from_env();

    let cwd = env::current_dir().context("Getting current working directory")?;
    println!("Working directory: {}", cwd.display());

    // Create timestamped output directory (absolute path)
    let output_dir = create_output_dir()?;
    println!("Created output directory: {}", output_dir);

    // Local-staging: copy the source onto local disk (the output_dir lives on
    // the container's local fs, e.g. tmpfs) so decode reads from local storage
    // instead of a network mount. Output is likewise written locally and copied
    // back at the end (handled by the non-direct-write path below).
    if args.local_stage && !args.source.is_empty() {
        let ext = Path::new(&args.source)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("mp4");
        let staged_source = format!("{}/staged_input.{}", output_dir, ext);
        metrics::time("stage_in", || copy_to_output(&args.source, &staged_source))?;
        println!("Staged source locally: {}", staged_source);
        args.source = staged_source;
    }

    // When output_filepath is set and we're not adding captions, write directly there so we
    // avoid the copy step and any temp-file behavior in the video library (usls) that can leave
    // the file missing at the expected temp path (e.g. on GCS FUSE). With --local-stage we
    // deliberately skip this direct write so the encode goes to local disk first.
    let processed_video = if !args.add_captions && !args.output_filepath.is_empty() && !args.local_stage {
        if let Some(parent) = Path::new(&args.output_filepath).parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Creating output directory {}", parent.display()))?;
        }
        println!("Writing processed video directly to: {}", args.output_filepath);
        args.output_filepath.clone()
    } else {
        format!("{}/processed_video.mp4", output_dir)
    };

    // If adding captions, prepare audio/transcription artifacts first
    let (extracted_audio, srt_path) = if args.add_captions {
        // Verify ffmpeg is installed
        audio::check_ffmpeg_installed()?;

        let extracted_audio = format!("{}/extracted_audio.mp4", output_dir);
        let compressed_audio = format!("{}/compressed_audio.mp3", output_dir);
        let srt_path = format!("{}/transcript.srt", output_dir);

        // Extract audio from the source video
        metrics::time("audio_extract", || audio::extract_audio(&args.source, &extracted_audio))?;
        println!("Audio extracted successfully to: {}", extracted_audio);

        // Compress the extracted audio to MP3
        metrics::time("audio_compress", || audio::compress_to_mp3(&extracted_audio, &compressed_audio))?;
        println!("Audio compressed to MP3: {}", compressed_audio);

        // Transcribe audio
        println!("Transcribing audio to: {}", srt_path);
        let transcript_config = transcript::TranscriptConfig::default();
        let transcribe_start = std::time::Instant::now();
        transcript::transcribe_audio(
            Path::new(&compressed_audio),
            Path::new(&srt_path),
            &transcript_config,
        )
        .await?;
        metrics::record("transcribe", transcribe_start.elapsed());
        println!("Transcription completed successfully");

        (Some(extracted_audio), Some(srt_path))
    } else {
        (None, None)
    };

    // Choose processor based on object type and smoothing preference
    metrics::time("process_video", || -> Result<()> {
        if args.object == "ball" {
            let mut processor = ball_video_processor::BallVideoProcessor::new(&args);
            processor.process_video(&args, &processed_video)
        } else if args.use_simple_smoothing {
            let mut processor =
                simple_smoothing_video_processor::SimpleSmoothingVideoProcessor::new();
            processor.process_video(&args, &processed_video)
        } else {
            let mut processor =
                history_smoothing_video_processor::HistorySmoothingVideoProcessor::new(&args);
            processor.process_video(&args, &processed_video)
        }
    })?;

    if args.add_captions {
        let captioned_video = format!("{}/captioned_video.mp4", output_dir);
        let final_video = format!("{}/final_output.mp4", output_dir);

        // Burn captions into the video
        println!("Burning captions into video...");
        let caption_style = audio::CaptionStyle::default();
        metrics::time("burn_captions", || {
            audio::burn_captions(
                &processed_video,
                &srt_path.as_ref().unwrap(),
                &captioned_video,
                Some(caption_style),
            )
        })?;
        println!("Captions burned successfully");

        // Add audio to the final video
        println!("Adding audio to video...");
        metrics::time("combine_av", || {
            audio::combine_video_audio(
                &captioned_video,
                &extracted_audio.as_ref().unwrap(),
                &final_video,
            )
        })?;
        println!(
            "Audio added successfully. Final video saved to: {}",
            final_video
        );

        // Copy final video to output_filepath if specified
        if !args.output_filepath.is_empty() {
            metrics::time("stage_out", || copy_to_output(&final_video, &args.output_filepath))?;
            println!(
                "Final video copied successfully to: {}",
                args.output_filepath
            );
        }
        // Ensure the output is flushed to GCS before exiting
        let final_path = if !args.output_filepath.is_empty() {
            &args.output_filepath
        } else {
            &final_video
        };
        sync_output_file(final_path)?;
    } else {
        println!("Processed video saved to: {}", processed_video);

        // Copy only when we wrote to temp and a destination is set (direct write path skips copy)
        if !args.output_filepath.is_empty() && processed_video != args.output_filepath {
            metrics::time("stage_out", || copy_to_output(&processed_video, &args.output_filepath))?;
            println!(
                "Processed video copied successfully to: {}",
                args.output_filepath
            );
        }
        // Ensure the output is flushed to GCS before exiting
        let final_path = if !args.output_filepath.is_empty() {
            &args.output_filepath
        } else {
            &processed_video
        };
        sync_output_file(final_path)?;
    }

    // Write the performance report next to the run artifacts, and (when an
    // output filepath is set, e.g. on GCS) next to the delivered video so
    // benchmark tooling can fetch it from the bucket.
    let run_metrics = format!("{}/metrics.json", output_dir);
    let mut metrics_paths: Vec<&str> = vec![&run_metrics];
    let delivered_metrics;
    if !args.output_filepath.is_empty() {
        delivered_metrics = format!("{}.metrics.json", args.output_filepath);
        metrics_paths.push(&delivered_metrics);
    }
    metrics::write_report(&metrics_paths)?;

    Ok(())
}
