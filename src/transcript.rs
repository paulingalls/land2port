use anyhow::{Result, anyhow};
use openai_api_rs::v1::audio::{AudioTranscriptionRequest, WHISPER_1};
use openai_api_rs::v1::api::OpenAIClient;
use std::path::Path;
use std::env;
use std::fs;

pub struct TranscriptConfig {
    pub api_key: String,
    pub model: String,
}

impl Default for TranscriptConfig {
    fn default() -> Self {
        Self {
            api_key: env::var("OPENAI_API_KEY").unwrap_or_default(),
            model: WHISPER_1.to_string(),
        }
    }
}

pub async fn transcribe_audio(audio_path: &Path, output_path: &Path, config: &TranscriptConfig) -> Result<()> {
    let mut client = OpenAIClient::builder()
        .with_api_key(&config.api_key)
        .build()
        .map_err(|e| anyhow!("Failed to create OpenAI client: {}", e))?;
    
    let mut request = AudioTranscriptionRequest::new(
        audio_path.to_string_lossy().to_string(),
        config.model.clone(),
    );
    request.response_format = Some("srt".to_string());

    let response = client.audio_transcription_raw(request)
        .await
        .map_err(|e| anyhow!("Failed to transcribe audio: {}", e))?;
    
    let srt_content = String::from_utf8_lossy(&response).to_string();
    
    // Create parent directories if they don't exist
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| anyhow!("Failed to create output directory: {}", e))?;
    }
    
    // Write the SRT content to the file
    fs::write(output_path, srt_content)
        .map_err(|e| anyhow!("Failed to write SRT file: {}", e))?;

    Ok(())
} 