use std::path::Path;
use crate::error::{WatchError, Result};
use crate::output::TranscriptSegment;

pub async fn transcribe_groq(audio_path: &Path, api_key: &str) -> Result<Vec<TranscriptSegment>> {
    let audio_bytes = std::fs::read(audio_path).map_err(|e| WatchError::Whisper(format!("Failed to read audio: {}", e)))?;
    let client = reqwest::Client::new();
    let part = reqwest::multipart::Part::bytes(audio_bytes).file_name("audio.mp3").mime_str("audio/mpeg").unwrap();
    let form = reqwest::multipart::Form::new().part("file", part).text("model", "whisper-large-v3").text("language", "en").text("response_format", "verbose_json");
    let resp = client.post("https://api.groq.com/openai/v1/audio/transcriptions").header("Authorization", format!("Bearer {}", api_key)).multipart(form).send().await.map_err(|e| WatchError::Whisper(format!("Groq request failed: {}", e)))?;
    if !resp.status().is_success() { return Err(WatchError::Whisper(format!("Groq API error {}", resp.status()))); }
    let json: serde_json::Value = resp.json().await.map_err(|e| WatchError::Whisper(format!("Groq parse error: {}", e)))?;
    if let Some(segments) = json["segments"].as_array() {
        Ok(segments.iter().filter_map(|seg| Some(TranscriptSegment { start: seg["start"].as_f64()?, end: seg["end"].as_f64()?, text: seg["text"].as_str()?.to_string() })).collect())
    } else {
        Ok(vec![TranscriptSegment { start: 0.0, end: 0.0, text: json["text"].as_str().unwrap_or("").to_string() }])
    }
}

pub async fn transcribe_openai(audio_path: &Path, api_key: &str) -> Result<Vec<TranscriptSegment>> {
    let audio_bytes = std::fs::read(audio_path).map_err(|e| WatchError::Whisper(format!("Failed to read audio: {}", e)))?;
    let client = reqwest::Client::new();
    let part = reqwest::multipart::Part::bytes(audio_bytes).file_name("audio.mp3").mime_str("audio/mpeg").unwrap();
    let form = reqwest::multipart::Form::new().part("file", part).text("model", "whisper-1").text("language", "en").text("response_format", "verbose_json");
    let resp = client.post("https://api.openai.com/v1/audio/transcriptions").header("Authorization", format!("Bearer {}", api_key)).multipart(form).send().await.map_err(|e| WatchError::Whisper(format!("OpenAI request failed: {}", e)))?;
    if !resp.status().is_success() { return Err(WatchError::Whisper(format!("OpenAI API error {}", resp.status()))); }
    let json: serde_json::Value = resp.json().await.map_err(|e| WatchError::Whisper(format!("OpenAI parse error: {}", e)))?;
    if let Some(segments) = json["segments"].as_array() {
        Ok(segments.iter().filter_map(|seg| Some(TranscriptSegment { start: seg["start"].as_f64()?, end: seg["end"].as_f64()?, text: seg["text"].as_str()?.to_string() })).collect())
    } else {
        Ok(vec![TranscriptSegment { start: 0.0, end: 0.0, text: json["text"].as_str().unwrap_or("").to_string() }])
    }
}

pub fn extract_audio(video_path: &Path, out_dir: &Path) -> Result<std::path::PathBuf> {
    let audio_path = out_dir.join("audio.mp3");
    let status = std::process::Command::new("ffmpeg").args(["-i", video_path.to_str().unwrap(), "-vn", "-ac", "1", "-ar", "16000", "-b:a", "64k", "-y", audio_path.to_str().unwrap()]).status().map_err(|e| WatchError::Ffmpeg(format!("ffmpeg not found: {}", e)))?;
    if !status.success() { return Err(WatchError::Ffmpeg("Audio extraction failed".into())); }
    Ok(audio_path)
}
