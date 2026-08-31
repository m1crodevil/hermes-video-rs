use thiserror::Error;

#[derive(Error, Debug)]
pub enum WatchError {
    #[error("yt-dlp error: {0}")]
    Download(String),

    #[error(
        "YouTube denied the video stream (HTTP 403). Configure a PO-token provider or use --cookies-file /path/to/youtube-cookies.txt; captions alone cannot support visual verification."
    )]
    VideoAccessDenied,

    #[error("ffmpeg error: {0}")]
    Ffmpeg(String),

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
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn display_download() {
        let e = WatchError::Download("timeout".into());
        assert_eq!(e.to_string(), "yt-dlp error: timeout");
    }

    #[test]
    fn display_video_access_denied() {
        assert!(
            WatchError::VideoAccessDenied
                .to_string()
                .contains("HTTP 403")
        );
    }

    #[test]
    fn display_ffmpeg() {
        let e = WatchError::Ffmpeg("codec fail".into());
        assert_eq!(e.to_string(), "ffmpeg error: codec fail");
    }

    #[test]
    fn display_whisper() {
        let e = WatchError::Whisper("rate limit".into());
        assert_eq!(e.to_string(), "Whisper API error: rate limit");
    }

    #[test]
    fn display_config() {
        let e = WatchError::Config("bad key".into());
        assert_eq!(e.to_string(), "Config error: bad key");
    }

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let watch_err: WatchError = io_err.into();
        assert!(matches!(watch_err, WatchError::Io(_)));
        assert!(watch_err.to_string().contains("missing"));
    }

    #[test]
    fn from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let watch_err: WatchError = json_err.into();
        assert!(matches!(watch_err, WatchError::Json(_)));
    }

    #[test]
    fn sanitize_path_strips_directory() {
        let p = Path::new("/home/user/video.mp4");
        assert_eq!(sanitize_path(p), "video.mp4");
    }

    #[test]
    fn sanitize_path_root() {
        assert_eq!(sanitize_path(Path::new("/")), "/");
    }

    #[test]
    fn sanitize_path_no_slash() {
        assert_eq!(sanitize_path(Path::new("no-slash")), "no-slash");
    }

    #[test]
    fn sanitize_path_unicode() {
        let p = Path::new("/home/user/日本語ファイル.mp4");
        assert_eq!(sanitize_path(p), "日本語ファイル.mp4");
    }

    #[test]
    fn display_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let watch_err: WatchError = io_err.into();
        let msg = watch_err.to_string();
        assert!(msg.starts_with("IO error:"));
        assert!(msg.contains("denied"));
    }

    #[test]
    fn display_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid!").unwrap_err();
        let watch_err: WatchError = json_err.into();
        let msg = watch_err.to_string();
        assert!(msg.starts_with("JSON error:"));
    }
}
