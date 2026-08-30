use crate::error::{Result, WatchError};
use crate::output::TranscriptSegment;
use std::path::Path;
use std::time::Duration;

const RETRY_BASE_DELAY: f64 = 2.0;
const MAX_RETRIES: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WhisperBackend {
    Groq,
    OpenAi,
}

impl WhisperBackend {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "groq" => Some(Self::Groq),
            "openai" => Some(Self::OpenAi),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Groq => "Groq",
            Self::OpenAi => "OpenAI",
        }
    }

    fn endpoint(self) -> &'static str {
        match self {
            Self::Groq => "https://api.groq.com/openai/v1/audio/transcriptions",
            Self::OpenAi => "https://api.openai.com/v1/audio/transcriptions",
        }
    }

    fn model(self) -> &'static str {
        match self {
            Self::Groq => "whisper-large-v3",
            Self::OpenAi => "whisper-1",
        }
    }
}

pub async fn transcribe(
    backend: WhisperBackend,
    audio_path: &Path,
    api_key: &str,
) -> Result<Vec<TranscriptSegment>> {
    transcribe_at(
        backend.name(),
        backend.endpoint(),
        backend.model(),
        audio_path,
        api_key,
    )
    .await
}

async fn transcribe_at(
    name: &str,
    endpoint: &str,
    model: &str,
    audio_path: &Path,
    api_key: &str,
) -> Result<Vec<TranscriptSegment>> {
    let audio_bytes = std::fs::read(audio_path).map_err(|e| {
        WatchError::Whisper(format!(
            "Failed to read audio '{}': {e}",
            audio_path.display()
        ))
    })?;
    let client = reqwest::Client::builder()
        .user_agent("hermes-video-rs/4.2")
        .build()
        .map_err(|e| WatchError::Whisper(format!("Failed to create HTTP client: {e}")))?;

    for attempt in 0..=MAX_RETRIES {
        let part = reqwest::multipart::Part::bytes(audio_bytes.clone())
            .file_name("audio.mp3")
            .mime_str("audio/mpeg")
            .expect("audio/mpeg is valid");
        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("model", model.to_string())
            .text("language", "en")
            .text("response_format", "verbose_json");
        let response = client
            .post(endpoint)
            .header("Authorization", format!("Bearer {api_key}"))
            .multipart(form)
            .send()
            .await
            .map_err(|e| WatchError::Whisper(format!("{name} request failed: {e}")))?;

        if response.status().as_u16() == 429 {
            if attempt < MAX_RETRIES {
                let delay = response
                    .headers()
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(Duration::from_secs)
                    .unwrap_or_else(|| {
                        Duration::from_secs((RETRY_BASE_DELAY * 2f64.powi(attempt as i32)) as u64)
                    });
                eprintln!(
                    "[watch2] rate limited by {name}, retrying in {}s ({}/{MAX_RETRIES})",
                    delay.as_secs(),
                    attempt + 1
                );
                tokio::time::sleep(delay).await;
                continue;
            }
            return Err(WatchError::Whisper(format!(
                "{name} API rate limit exceeded after {MAX_RETRIES} retries"
            )));
        }
        if !response.status().is_success() {
            return Err(WatchError::Whisper(format!(
                "{name} API error: HTTP {}",
                response.status()
            )));
        }
        let json = response
            .json()
            .await
            .map_err(|e| WatchError::Whisper(format!("{name} response parse error: {e}")))?;
        return parse_response(&json);
    }
    unreachable!()
}

fn parse_response(json: &serde_json::Value) -> Result<Vec<TranscriptSegment>> {
    if let Some(segments) = json["segments"].as_array() {
        Ok(segments
            .iter()
            .filter_map(|segment| {
                Some(TranscriptSegment {
                    start: segment["start"].as_f64()?,
                    end: segment["end"].as_f64()?,
                    text: segment["text"].as_str()?.to_string(),
                    words: None,
                })
            })
            .collect())
    } else {
        Ok(vec![TranscriptSegment {
            start: 0.0,
            end: 0.0,
            text: json["text"].as_str().unwrap_or_default().to_string(),
            words: None,
        }])
    }
}

pub fn extract_audio(video_path: &Path, out_dir: &Path) -> Result<std::path::PathBuf> {
    let audio_path = out_dir.join("audio.mp3");
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-i",
            video_path.to_str().unwrap_or_default(),
            "-vn",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-b:a",
            "64k",
            "-y",
            audio_path.to_str().unwrap_or_default(),
        ])
        .status()
        .map_err(|e| WatchError::Ffmpeg(format!("ffmpeg not found: {e}")))?;
    if !status.success() {
        return Err(WatchError::Ffmpeg("Audio extraction failed".into()));
    }
    Ok(audio_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_selects_known_services() {
        assert_eq!(
            WhisperBackend::from_name("groq"),
            Some(WhisperBackend::Groq)
        );
        assert_eq!(
            WhisperBackend::from_name("openai"),
            Some(WhisperBackend::OpenAi)
        );
        assert_eq!(WhisperBackend::from_name("unknown"), None);
    }

    #[test]
    fn parses_segment_response() {
        let json = serde_json::json!({"segments": [{"start": 0.0, "end": 1.0, "text": "hello"}]});
        assert_eq!(parse_response(&json).unwrap()[0].text, "hello");
    }
}
