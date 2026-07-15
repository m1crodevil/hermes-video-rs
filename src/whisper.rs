use std::path::Path;
use std::time::Duration;
use async_trait::async_trait;
use crate::error::{WatchError, Result};
use crate::output::TranscriptSegment;

const RETRY_BASE_DELAY: f64 = 2.0;
const MAX_RETRIES: u32 = 4;

// ── Provider Trait ───────────────────────────────────────────────────────

/// Trait for Whisper-compatible transcription providers.
#[async_trait]
pub trait WhisperProvider: Send + Sync {
    /// Provider name for logging.
    fn name(&self) -> &str;

    /// API endpoint URL.
    fn endpoint(&self) -> &str;

    /// Model identifier.
    fn model(&self) -> &str;

    /// Transcribe an audio file to transcript segments.
    async fn transcribe(&self, audio_path: &Path, api_key: &str) -> Result<Vec<TranscriptSegment>> {
        let audio_bytes = std::fs::read(audio_path).map_err(|e| {
            WatchError::Whisper(format!("Failed to read audio '{}': {}", audio_path.display(), e))
        })?;

        let client = reqwest::Client::builder()
            .user_agent("hermes-video-rs/4.2")
            .build()
            .map_err(|e| WatchError::Whisper(format!("Failed to create HTTP client: {}", e)))?;

        for attempt in 0..=MAX_RETRIES {
            let part = reqwest::multipart::Part::bytes(audio_bytes.clone())
                .file_name("audio.mp3")
                .mime_str("audio/mpeg")
                .unwrap();

            let form = reqwest::multipart::Form::new()
                .part("file", part)
                .text("model", self.model().to_string())
                .text("language", "en")
                .text("response_format", "verbose_json");

            let resp = client
                .post(self.endpoint())
                .header("Authorization", format!("Bearer {}", api_key))
                .multipart(form)
                .send()
                .await
                .map_err(|e| {
                    WatchError::Whisper(format!("{} request failed: {}", self.name(), e))
                })?;

            // Handle rate limiting (HTTP 429) with exponential backoff
            if resp.status().as_u16() == 429 {
                if attempt < MAX_RETRIES {
                    let delay = resp
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok())
                        .map(Duration::from_secs)
                        .unwrap_or_else(|| {
                            Duration::from_secs(
                                (RETRY_BASE_DELAY * 2f64.powi(attempt as i32)) as u64,
                            )
                        });
                    eprintln!(
                        "[watch2] rate limited by {} API, retrying in {}s (attempt {}/{})...",
                        self.name(),
                        delay.as_secs(),
                        attempt + 1,
                        MAX_RETRIES
                    );
                    tokio::time::sleep(delay).await;
                    continue;
                }
                return Err(WatchError::Whisper(format!(
                    "{} API rate limit exceeded after {} retries",
                    self.name(),
                    MAX_RETRIES
                )));
            }

            if !resp.status().is_success() {
                return Err(WatchError::Whisper(format!(
                    "{} API error: HTTP {}",
                    self.name(),
                    resp.status()
                )));
            }

            let json: serde_json::Value = resp.json().await.map_err(|e| {
                WatchError::Whisper(format!("{} response parse error: {}", self.name(), e))
            })?;

            return parse_response(&json);
        }

        unreachable!()
    }
}

// ── Response Parsing ─────────────────────────────────────────────────────

fn parse_response(json: &serde_json::Value) -> Result<Vec<TranscriptSegment>> {
    if let Some(segments) = json["segments"].as_array() {
        Ok(segments
            .iter()
            .filter_map(|seg| {
                Some(TranscriptSegment {
                    start: seg["start"].as_f64()?,
                    end: seg["end"].as_f64()?,
                    text: seg["text"].as_str()?.to_string(),
                    words: None,
                })
            })
            .collect())
    } else {
        Ok(vec![TranscriptSegment {
            start: 0.0,
            end: 0.0,
            text: json["text"].as_str().unwrap_or("").to_string(),
            words: None,
        }])
    }
}

// ── Concrete Providers ───────────────────────────────────────────────────

/// Groq Whisper API provider.
pub struct GroqProvider;

#[async_trait]
impl WhisperProvider for GroqProvider {
    fn name(&self) -> &str {
        "Groq"
    }

    fn endpoint(&self) -> &str {
        "https://api.groq.com/openai/v1/audio/transcriptions"
    }

    fn model(&self) -> &str {
        "whisper-large-v3"
    }
}

/// OpenAI Whisper API provider.
pub struct OpenAIProvider;

#[async_trait]
impl WhisperProvider for OpenAIProvider {
    fn name(&self) -> &str {
        "OpenAI"
    }

    fn endpoint(&self) -> &str {
        "https://api.openai.com/v1/audio/transcriptions"
    }

    fn model(&self) -> &str {
        "whisper-1"
    }
}

// ── Factory ──────────────────────────────────────────────────────────────

/// Create a Whisper provider by name ("groq" or "openai").
pub fn create_provider(backend: &str) -> Box<dyn WhisperProvider> {
    match backend {
        "groq" => Box::new(GroqProvider),
        _ => Box::new(OpenAIProvider),
    }
}

// ── Audio Extraction ─────────────────────────────────────────────────────

pub fn extract_audio(video_path: &Path, out_dir: &Path) -> Result<std::path::PathBuf> {
    let audio_path = out_dir.join("audio.mp3");
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-i",
            video_path.to_str().unwrap(),
            "-vn",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-b:a",
            "64k",
            "-y",
            audio_path.to_str().unwrap(),
        ])
        .status()
        .map_err(|e| WatchError::Ffmpeg(format!("ffmpeg not found: {}", e)))?;
    if !status.success() {
        return Err(WatchError::Ffmpeg("Audio extraction failed".into()));
    }
    Ok(audio_path)
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_names() {
        assert_eq!(GroqProvider.name(), "Groq");
        assert_eq!(OpenAIProvider.name(), "OpenAI");
    }

    #[test]
    fn test_provider_endpoints() {
        assert!(GroqProvider.endpoint().contains("groq.com"));
        assert!(OpenAIProvider.endpoint().contains("openai.com"));
    }

    #[test]
    fn test_provider_models() {
        assert_eq!(GroqProvider.model(), "whisper-large-v3");
        assert_eq!(OpenAIProvider.model(), "whisper-1");
    }

    #[test]
    fn test_create_provider() {
        let p = create_provider("groq");
        assert_eq!(p.name(), "Groq");
        let p = create_provider("openai");
        assert_eq!(p.name(), "OpenAI");
        let p = create_provider("unknown");
        assert_eq!(p.name(), "OpenAI"); // default
    }

    #[test]
    fn test_parse_response_with_segments() {
        let json = serde_json::json!({
            "segments": [
                {"start": 0.0, "end": 1.5, "text": "Hello world"},
                {"start": 1.5, "end": 3.0, "text": "How are you?"}
            ]
        });
        let segs = parse_response(&json).unwrap();
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].text, "Hello world");
        assert_eq!(segs[1].start, 1.5);
    }

    #[test]
    fn test_parse_response_text_only() {
        let json = serde_json::json!({
            "text": "Just a single text block"
        });
        let segs = parse_response(&json).unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "Just a single text block");
        assert_eq!(segs[0].start, 0.0);
    }
}
