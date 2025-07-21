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
    /// Outline color in hex format (e.g., "000000" for black)
    pub outline_color: Option<String>,
    /// Outline thickness in pixels
    pub outline_thickness: Option<u32>,
    /// Shadow color in hex format (e.g., "000000" for black)
    pub shadow_color: Option<String>,
    /// Shadow distance in pixels
    pub shadow_distance: Option<u32>,
}

impl Default for CaptionStyle {
    fn default() -> Self {
        Self {
            font_size: 8,
            font_color: "FFFFFF".to_string(),
            font_name: "Arial".to_string(),
            h_align: "center".to_string(),
            margin_bottom: 20,  // 20 pixels from bottom
            bg_color: None,
            bg_opacity: None,
            outline_color: Some("000000".to_string()),
            outline_thickness: Some(1),
            shadow_color: None,
            shadow_distance: None,
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

    // Determine BorderStyle based on what's specified (check before moving values)
    let has_background = style.bg_color.is_some() || style.bg_opacity.is_some();
    let has_outline = style.outline_color.is_some() || style.outline_thickness.is_some();
    let has_shadow = style.shadow_color.is_some() || style.shadow_distance.is_some();

    // Add background color and opacity if specified
    if let (Some(bg_color), Some(opacity)) = (style.bg_color, style.bg_opacity) {
        // Convert opacity to hex (0-255)
        let opacity_hex = format!("{:02X}", (opacity * 255.0) as u8);
        // Format background color with opacity
        let bg_color_with_opacity = format!("{}{}", opacity_hex, bg_color);
        
        filter_str.push_str(&format!(
            ",BackColour=&H{}",
            bg_color_with_opacity
        ));
    }

    
    // Add outline color and thickness if specified
    if let Some(outline_color) = style.outline_color {
        filter_str.push_str(&format!(
            ",OutlineColour=&H{}",
            outline_color
        ));
    }

    if let Some(outline_thickness) = style.outline_thickness {
        filter_str.push_str(&format!(
            ",Outline={}",
            outline_thickness
        ));
    }

    // Add shadow color and distance if specified
    if let Some(shadow_color) = style.shadow_color {
        filter_str.push_str(&format!(
            ",ShadowColour=&H{}",
            shadow_color
        ));
    }

    if let Some(shadow_distance) = style.shadow_distance {
        filter_str.push_str(&format!(
            ",Shadow={}",
            shadow_distance
        ));
    }
    
    let border_style = match (has_background, has_outline, has_shadow) {
        (true, _, _) => 3,
        (false, true, true) => 1,
        (false, true, false) => 1,
        (false, false, true) => 1,
        (false, false, false) => 0,
    };
    
    filter_str.push_str(&format!(",BorderStyle={}", border_style));
    filter_str.push('\'');

    println!("filter_str: {}", filter_str);

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
            "-q:a", "6",  // Set quality (2 is high quality, range is 0-9 where lower is better)
            output_path,
        ])
        .status()
        .context("Failed to execute ffmpeg command to compress audio")?;

    if !status.success() {
        anyhow::bail!("ffmpeg command failed with status: {}", status);
    }

    Ok(())
} 