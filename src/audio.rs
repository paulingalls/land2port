use anyhow::{Context, Result};
use std::process::Command;

/// Configuration options for caption styling and positioning
#[derive(Debug, Clone)]
pub struct CaptionStyle {
    /// Font size in pixels
    pub font_size: u32,
    /// Font color in hex format (e.g., "FFFFFF" for white)
    pub font_color: String,
    /// Font name (e.g., "Arial", "Helvetica")
    pub font_name: String,
    /// Horizontal alignment: "left", "center", or "right"
    pub h_align: String,
    /// Margin from bottom in pixels
    pub margin_bottom: u32,
    /// Background color in hex format (e.g., "000000" for black)
    pub bg_color: Option<String>,
    /// Background opacity (0.0 to 1.0)
    pub bg_opacity: Option<f32>,
}

impl Default for CaptionStyle {
    fn default() -> Self {
        Self {
            font_size: 10,
            font_color: "FFFFFF".to_string(),
            font_name: "Arial".to_string(),
            h_align: "center".to_string(),
            margin_bottom: 20,  // 20 pixels from bottom
            bg_color: Some("000000".to_string()),
            bg_opacity: Some(0.5),
        }
    }
}

/// Extracts audio from a video file using ffmpeg
pub fn extract_audio(video_path: &str, output_path: &str) -> Result<()> {
    let status = Command::new("ffmpeg")
        .args([
            "-i", video_path,
            "-vn",  // Disable video
            "-acodec", "copy",  // Copy audio stream without re-encoding
            output_path,
        ])
        .status()
        .context("Failed to execute ffmpeg command")?;

    if !status.success() {
        anyhow::bail!("ffmpeg command failed with status: {}", status);
    }

    Ok(())
}

/// Checks if ffmpeg is installed and available in the system
pub fn check_ffmpeg_installed() -> Result<()> {
    let status = Command::new("ffmpeg")
        .arg("-version")
        .status()
        .context("Failed to execute ffmpeg command. Is ffmpeg installed?")?;

    if !status.success() {
        anyhow::bail!("ffmpeg is not properly installed");
    }

    Ok(())
}

/// Burns SRT captions into a video file using ffmpeg with customizable styling
pub fn burn_captions(
    video_path: &str,
    srt_path: &str,
    output_path: &str,
    style: Option<CaptionStyle>,
) -> Result<()> {
    let style = style.unwrap_or_default();
    
    // Build the subtitle filter string with styling options
    let mut filter_str = format!(
        "subtitles={}:force_style='FontName={},FontSize={},PrimaryColour=&H{},Alignment={},MarginV={}",
        srt_path,
        style.font_name,
        style.font_size,
        style.font_color,
        match style.h_align.as_str() {
            "left" => "1",
            "center" => "2",
            "right" => "3",
            _ => "1",
        },
        style.margin_bottom
    );

    // Add background color and opacity if specified
    if let (Some(bg_color), Some(opacity)) = (style.bg_color, style.bg_opacity) {
        // Convert opacity to hex (0-255)
        let opacity_hex = format!("{:02X}", (opacity * 255.0) as u8);
        // Format background color with opacity
        let bg_color_with_opacity = format!("{}{}", opacity_hex, bg_color);
        
        filter_str.push_str(&format!(
            ",BackColour=&H{},OutlineColour=&H{},BorderStyle=3,Outline=1,Shadow=0",
            bg_color_with_opacity,
            bg_color_with_opacity
        ));
    }

    filter_str.push('\'');

    let status = Command::new("ffmpeg")
        .args([
            "-i", video_path,
            "-vf", &filter_str,
            "-c:a", "copy",  // Copy audio stream without re-encoding
            output_path,
        ])
        .status()
        .context("Failed to execute ffmpeg command to burn captions")?;

    if !status.success() {
        anyhow::bail!("ffmpeg command failed with status: {}", status);
    }

    Ok(())
}

/// Combines a video file with an audio file into a new video file
pub fn combine_video_audio(
    video_path: &str,
    audio_path: &str,
    output_path: &str,
) -> Result<()> {
    let status = Command::new("ffmpeg")
        .args([
            "-i", video_path,  // Input video
            "-i", audio_path,  // Input audio
            "-c:v", "copy",    // Copy video stream without re-encoding
            "-c:a", "copy",    // Copy audio stream without re-encoding
            "-map", "0:v:0",   // Use video from first input
            "-map", "1:a:0",   // Use audio from second input
            "-shortest",       // End when shortest input ends
            output_path,
        ])
        .status()
        .context("Failed to execute ffmpeg command to combine video and audio")?;

    if !status.success() {
        anyhow::bail!("ffmpeg command failed with status: {}", status);
    }

    Ok(())
}

/// Compresses an audio file from MP4 format to MP3 format using ffmpeg
pub fn compress_to_mp3(input_path: &str, output_path: &str) -> Result<()> {
    let status = Command::new("ffmpeg")
        .args([
            "-i", input_path,
            "-vn",  // Disable video
            "-acodec", "libmp3lame",  // Use MP3 codec
            "-q:a", "5",  // Set quality (2 is high quality, range is 0-9 where lower is better)
            output_path,
        ])
        .status()
        .context("Failed to execute ffmpeg command to compress audio")?;

    if !status.success() {
        anyhow::bail!("ffmpeg command failed with status: {}", status);
    }

    Ok(())
} 