use crate::error::{Result, WatchError};
use crate::output::TranscriptSegment;
use async_trait::async_trait;
use std::path::Path;
use std::time::Duration;

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
            WatchError::Whisper(format!(
                "Failed to read audio '{}': {}",
                audio_path.display(),
                e
            ))
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
    use tempfile::NamedTempFile;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // ── Mock Provider for wiremock tests ──────────────────────────────

    /// A mock WhisperProvider that points at a wiremock MockServer URL.
    struct MockWhisperProvider {
        endpoint_url: String,
        model_id: String,
    }

    #[async_trait]
    impl WhisperProvider for MockWhisperProvider {
        fn name(&self) -> &str {
            "MockGroq"
        }

        fn endpoint(&self) -> &str {
            &self.endpoint_url
        }

        fn model(&self) -> &str {
            &self.model_id
        }
    }

    impl MockWhisperProvider {
        fn new(server: &MockServer) -> Self {
            let endpoint_url = format!("{}/openai/v1/audio/transcriptions", server.uri());
            Self {
                endpoint_url,
                model_id: "whisper-large-v3".to_string(),
            }
        }
    }

    /// Create a small temp file with fake audio bytes.
    fn create_temp_audio() -> NamedTempFile {
        let file = NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"fake-audio-bytes-for-testing").unwrap();
        file
    }

    // ── Provider unit tests ───────────────────────────────────────────

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

    // ── parse_response unit tests ─────────────────────────────────────

    #[test]
    fn test_parse_response_segments() {
        let json = serde_json::json!({
            "segments": [
                {"start": 0.0, "end": 1.5, "text": "Hello world"},
                {"start": 1.5, "end": 3.0, "text": "How are you?"}
            ]
        });
        let segs = parse_response(&json).unwrap();
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].text, "Hello world");
        assert_eq!(segs[0].start, 0.0);
        assert_eq!(segs[0].end, 1.5);
        assert_eq!(segs[1].text, "How are you?");
        assert_eq!(segs[1].start, 1.5);
        assert_eq!(segs[1].end, 3.0);
        // words should be None
        assert!(segs[0].words.is_none());
        assert!(segs[1].words.is_none());
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
        assert_eq!(segs[0].end, 0.0);
        assert!(segs[0].words.is_none());
    }

    #[test]
    fn test_parse_response_empty_segments() {
        let json = serde_json::json!({
            "segments": []
        });
        let segs = parse_response(&json).unwrap();
        assert_eq!(segs.len(), 0);
    }

    // ── Wiremock integration tests ────────────────────────────────────

    #[tokio::test]
    async fn test_transcribe_success() {
        let server = MockServer::start().await;
        let provider = MockWhisperProvider::new(&server);

        let response_body = serde_json::json!({
            "segments": [
                {"start": 0.0, "end": 2.0, "text": "Hello from Groq"},
                {"start": 2.0, "end": 4.5, "text": "This is a test"}
            ]
        });

        Mock::given(method("POST"))
            .and(path("/openai/v1/audio/transcriptions"))
            .and(header("Authorization", "Bearer test-api-key"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(response_body)
                    .insert_header("content-type", "application/json"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let audio_file = create_temp_audio();
        let segments = provider
            .transcribe(audio_file.path(), "test-api-key")
            .await
            .unwrap();

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "Hello from Groq");
        assert_eq!(segments[0].start, 0.0);
        assert_eq!(segments[0].end, 2.0);
        assert_eq!(segments[1].text, "This is a test");
        assert_eq!(segments[1].start, 2.0);
        assert_eq!(segments[1].end, 4.5);
    }

    #[tokio::test]
    async fn test_transcribe_rate_limit_retry() {
        let server = MockServer::start().await;
        let provider = MockWhisperProvider::new(&server);

        let success_body = serde_json::json!({
            "segments": [
                {"start": 0.0, "end": 1.0, "text": "Retry succeeded"}
            ]
        });

        // First request: 429 with Retry-After: 0
        Mock::given(method("POST"))
            .and(path("/openai/v1/audio/transcriptions"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("Retry-After", "0")
                    .set_body_string("rate limited"),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;

        // Second request: 200 OK
        Mock::given(method("POST"))
            .and(path("/openai/v1/audio/transcriptions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(success_body)
                    .insert_header("content-type", "application/json"),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;

        let audio_file = create_temp_audio();
        let segments = provider
            .transcribe(audio_file.path(), "test-api-key")
            .await
            .unwrap();

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "Retry succeeded");
    }

    #[tokio::test]
    async fn test_transcribe_rate_limit_exhausted() {
        let server = MockServer::start().await;
        let provider = MockWhisperProvider::new(&server);

        // All attempts return 429 (MAX_RETRIES + 1 = 5 attempts)
        Mock::given(method("POST"))
            .and(path("/openai/v1/audio/transcriptions"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("Retry-After", "0")
                    .set_body_string("rate limited"),
            )
            .expect(5) // 0..=MAX_RETRIES = 5 attempts
            .mount(&server)
            .await;

        let audio_file = create_temp_audio();
        let err_msg = match provider.transcribe(audio_file.path(), "test-api-key").await {
            Ok(_) => panic!("expected rate limit error"),
            Err(e) => e.to_string(),
        };

        assert!(
            err_msg.contains("rate limit exceeded"),
            "Error should mention rate limit: {err_msg}"
        );
        assert!(
            err_msg.contains("4 retries"),
            "Error should mention retry count: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_transcribe_server_error() {
        let server = MockServer::start().await;
        let provider = MockWhisperProvider::new(&server);

        Mock::given(method("POST"))
            .and(path("/openai/v1/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(500).set_body_string("internal server error"))
            .expect(1)
            .mount(&server)
            .await;

        let audio_file = create_temp_audio();
        let err_msg = match provider.transcribe(audio_file.path(), "test-api-key").await {
            Ok(_) => panic!("expected server error"),
            Err(e) => e.to_string(),
        };

        assert!(
            err_msg.contains("HTTP 500"),
            "Error should mention HTTP 500: {err_msg}"
        );
    }
}
