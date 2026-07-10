use std::path::{Path, PathBuf};
use std::process::Command;
use crate::error::{WatchError, Result};

pub struct DownloadResult {
    pub video_path: Option<PathBuf>,
    pub subtitle_path: Option<PathBuf>,
    pub title: String,
    pub downloaded: bool,
}

pub fn is_url(source: &str) -> bool {
    source.starts_with("http://") || source.starts_with("https://")
}

pub fn resolve_local(path: &str) -> Result<DownloadResult> {
    let p = Path::new(path).canonicalize()
        .map_err(|_| WatchError::Download(format!("File not found: {}", path)))?;
    Ok(DownloadResult {
        video_path: Some(p.clone()),
        subtitle_path: None,
        title: p.file_name().unwrap_or_default().to_string_lossy().to_string(),
        downloaded: false,
    })
}

pub fn fetch_captions(url: &str, out_dir: &Path) -> Result<DownloadResult> {
    std::fs::create_dir_all(out_dir)?;
    let output_template = out_dir.join("video.%(ext)s").to_string_lossy().to_string();
    
    let status = Command::new("yt-dlp")
        .args([
            "--skip-download",
            "--write-info-json",
            "--write-subs",
            "--write-auto-subs",
            "--sub-langs", "en.*",
            "--sub-format", "json3/best",
            "--no-playlist",
            "--ignore-errors",
            "--sleep-subtitles", "3",
            "-o", &output_template,
            "--", url,
        ])
        .status();
    
    match status {
        Ok(s) if s.success() => {
            let subtitle_path = find_subtitle(out_dir);
            let title = extract_title(out_dir);
            Ok(DownloadResult {
                video_path: None,
                subtitle_path,
                title,
                downloaded: false,
            })
        }
        Ok(_) => Err(WatchError::Download("yt-dlp caption fetch failed".into())),
        Err(e) => Err(WatchError::Download(format!("yt-dlp not found: {}", e))),
    }
}

pub fn download_video(url: &str, out_dir: &Path) -> Result<DownloadResult> {
    std::fs::create_dir_all(out_dir)?;
    let output_template = out_dir.join("video.%(ext)s").to_string_lossy().to_string();
    
    let status = Command::new("yt-dlp")
        .args([
            "--write-subs",
            "--write-auto-subs",
            "--sub-langs", "en.*",
            "--sub-format", "json3/best",
            "--no-playlist",
            "--ignore-errors",
            "--sleep-subtitles", "3",
            "-o", &output_template,
            "--", url,
        ])
        .status();
    
    match status {
        Ok(s) if s.success() => {
            let video_path = find_video(out_dir);
            let subtitle_path = find_subtitle(out_dir);
            let title = extract_title(out_dir);
            Ok(DownloadResult {
                video_path,
                subtitle_path,
                title,
                downloaded: true,
            })
        }
        Ok(_) => Err(WatchError::Download("yt-dlp download failed".into())),
        Err(e) => Err(WatchError::Download(format!("yt-dlp not found: {}", e))),
    }
}

fn find_video(dir: &Path) -> Option<PathBuf> {
    for ext in &[".mp4", ".mkv", ".webm", ".mov", ".m4a", ".mp3"] {
        for entry in std::fs::read_dir(dir).ok()? {
            let entry = entry.ok()?;
            if entry.path().extension().map_or(false, |e| e == *ext) {
                return Some(entry.path());
            }
        }
    }
    None
}

fn find_subtitle(dir: &Path) -> Option<PathBuf> {
    for ext in &[".json3", ".vtt"] {
        for entry in std::fs::read_dir(dir).ok()? {
            let entry = entry.ok()?;
            if entry.path().extension().map_or(false, |e| e == *ext) {
                return Some(entry.path());
            }
        }
    }
    None
}

fn extract_title(dir: &Path) -> String {
    for entry in std::fs::read_dir(dir).ok() {
        for entry in entry.flatten() {
            if entry.path().extension().map_or(false, |e| e == "json") {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(title) = json["title"].as_str() {
                            return title.to_string();
                        }
                    }
                }
            }
        }
    }
    "Unknown".to_string()
}
