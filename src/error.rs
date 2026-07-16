use thiserror::Error;

#[derive(Error, Debug)]
pub enum WatchError {
    #[error("yt-dlp error: {0}")]
    Download(String),

    #[error("ffmpeg error: {0}")]
    Ffmpeg(String),

    #[error("No captions available for this video")]
    NoCaptions,

    #[error("Whisper API error: {0}")]
    Whisper(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, WatchError>;

/// Sanitize file path for user-facing error messages
/// Shows only filename, not full path (prevents information disclosure)
pub fn sanitize_path(path: &std::path::Path) -> String {
    path.file_name()
        .unwrap_or_else(|| path.as_os_str())
        .to_string_lossy()
        .to_string()
}
