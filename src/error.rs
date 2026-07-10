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
