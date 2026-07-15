use std::path::Path;
use std::time::Duration;
use crate::error::{WatchError, Result};
use crate::output::TranscriptSegment;

const RETRY_BASE_DELAY: f64 = 2.0;

pub async fn transcribe_groq(audio_path: &Path, api_key: &str) -> Result<Vec<TranscriptSegment>> {
    let audio_bytes = std::fs::read(audio_path)
        .map_err(|e| WatchError::Whisper(format!("Failed to read audio '{}': {}", audio_path.display(), e)))?;
    let client = reqwest::Client::builder()
        .user_agent("hermes-video-rs/3.2")
        .build()
        .map_err(|e| WatchError::Whisper(format!("Failed to create HTTP client: {}", e)))?;
    let max_retries = 4u32;

    for attempt in 0..=max_retries {
        let part = reqwest::multipart::Part::bytes(audio_bytes.clone())
            .file_name("audio.mp3")
            .mime_str("audio/mpeg")
            .unwrap();
        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("model", "whisper-large-v3")
            .text("language", "en")
            .text("response_format", "verbose_json");

        let resp = client.post("https://api.groq.com/openai/v1/audio/transcriptions")
            .header("Authorization", format!("Bearer {}", api_key))
            .multipart(form)
            .send()
            .await
            .map_err(|e| WatchError::Whisper(format!("Groq request failed: {}", e)))?;

        // Handle rate limiting (HTTP 429) with exponential backoff
        if resp.status().as_u16() == 429 {
            if attempt < max_retries {
                let delay = resp.headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .map(Duration::from_secs)
                    .unwrap_or_else(|| {
                        Duration::from_secs((RETRY_BASE_DELAY * 2f64.powi(attempt as i32)) as u64)
                    });
                eprintln!("[watch2] rate limited by Groq API, retrying in {}s (attempt {}/{})...", delay.as_secs(), attempt + 1, max_retries);
                tokio::time::sleep(delay).await;
                continue;
            }
            return Err(WatchError::Whisper(format!("Groq API rate limit exceeded after {} retries", max_retries)));
        }

        if !resp.status().is_success() {
            return Err(WatchError::Whisper(format!("Groq API error: HTTP {}", resp.status())));
        }

        let json: serde_json::Value = resp.json().await
            .map_err(|e| WatchError::Whisper(format!("Groq response parse error: {}", e)))?;

        if let Some(segments) = json["segments"].as_array() {
            return Ok(segments.iter().filter_map(|seg| Some(TranscriptSegment {
                start: seg["start"].as_f64()?,
                end: seg["end"].as_f64()?,
                text: seg["text"].as_str()?.to_string(),
                words: None,
            })).collect());
        } else {
            return Ok(vec![TranscriptSegment {
                start: 0.0,
                end: 0.0,
                text: json["text"].as_str().unwrap_or("").to_string(),
                words: None,
            }]);
        }
    }

    unreachable!()
}

pub async fn transcribe_openai(audio_path: &Path, api_key: &str) -> Result<Vec<TranscriptSegment>> {
    let audio_bytes = std::fs::read(audio_path)
        .map_err(|e| WatchError::Whisper(format!("Failed to read audio '{}': {}", audio_path.display(), e)))?;
    let client = reqwest::Client::builder()
        .user_agent("hermes-video-rs/3.2")
        .build()
        .map_err(|e| WatchError::Whisper(format!("Failed to create HTTP client: {}", e)))?;
    let max_retries = 4u32;

    for attempt in 0..=max_retries {
        let part = reqwest::multipart::Part::bytes(audio_bytes.clone())
            .file_name("audio.mp3")
            .mime_str("audio/mpeg")
            .unwrap();
        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("model", "whisper-1")
            .text("language", "en")
            .text("response_format", "verbose_json");

        let resp = client.post("https://api.openai.com/v1/audio/transcriptions")
            .header("Authorization", format!("Bearer {}", api_key))
            .multipart(form)
            .send()
            .await
            .map_err(|e| WatchError::Whisper(format!("OpenAI request failed: {}", e)))?;

        // Handle rate limiting (HTTP 429) with exponential backoff
        if resp.status().as_u16() == 429 {
            if attempt < max_retries {
                let delay = resp.headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .map(Duration::from_secs)
                    .unwrap_or_else(|| {
                        Duration::from_secs((RETRY_BASE_DELAY * 2f64.powi(attempt as i32)) as u64)
                    });
                eprintln!("[watch2] rate limited by OpenAI API, retrying in {}s (attempt {}/{})...", delay.as_secs(), attempt + 1, max_retries);
                tokio::time::sleep(delay).await;
                continue;
            }
            return Err(WatchError::Whisper(format!("OpenAI API rate limit exceeded after {} retries", max_retries)));
        }

        if !resp.status().is_success() {
            return Err(WatchError::Whisper(format!("OpenAI API error: HTTP {}", resp.status())));
        }

        let json: serde_json::Value = resp.json().await
            .map_err(|e| WatchError::Whisper(format!("OpenAI response parse error: {}", e)))?;

        if let Some(segments) = json["segments"].as_array() {
            return Ok(segments.iter().filter_map(|seg| Some(TranscriptSegment {
                start: seg["start"].as_f64()?,
                end: seg["end"].as_f64()?,
                text: seg["text"].as_str()?.to_string(),
                words: None,
            })).collect());
        } else {
            return Ok(vec![TranscriptSegment {
                start: 0.0,
                end: 0.0,
                text: json["text"].as_str().unwrap_or("").to_string(),
                words: None,
            }]);
        }
    }

    unreachable!()
}

pub fn extract_audio(video_path: &Path, out_dir: &Path) -> Result<std::path::PathBuf> {
    let audio_path = out_dir.join("audio.mp3");
    let status = std::process::Command::new("ffmpeg").args(["-i", video_path.to_str().unwrap(), "-vn", "-ac", "1", "-ar", "16000", "-b:a", "64k", "-y", audio_path.to_str().unwrap()]).status().map_err(|e| WatchError::Ffmpeg(format!("ffmpeg not found: {}", e)))?;
    if !status.success() { return Err(WatchError::Ffmpeg("Audio extraction failed".into())); }
    Ok(audio_path)
}
