use std::path::{Path, PathBuf};
use std::process::Command;
use crate::error::{WatchError, Result};
use crate::config::{suggest_subtitle_language, get_language_name};

/// Rich metadata extracted from a video's info.json sidecar file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VideoInfo {
    pub title: String,
    pub uploader: Option<String>,
    pub duration: Option<f64>,
    pub language: Option<String>,
    pub description: Option<String>,
}

impl Default for VideoInfo {
    fn default() -> Self {
        Self {
            title: "Unknown".to_string(),
            uploader: None,
            duration: None,
            language: None,
            description: None,
        }
    }
}

pub struct DownloadResult {
    pub video_path: Option<PathBuf>,
    pub subtitle_path: Option<PathBuf>,
    pub title: String,
    pub info: VideoInfo,
    pub downloaded: bool,
}

// ---------------------------------------------------------------------------
// YouTube 2026 network opts
// ---------------------------------------------------------------------------

fn has_chrome_cookies() -> bool {
    let home = dirs::home_dir().unwrap_or_default();
    [
        home.join(".config/google-chrome/Default/Cookies"),
        home.join(".config/chromium/Default/Cookies"),
        home.join("Library/Application Support/Google/Chrome/Default/Cookies"),
    ]
    .iter()
    .any(|p| p.exists())
}

/// Network-related yt-dlp flags for YouTube 2026+ reliability.
///
/// YouTube now requires:
///   1. A JS runtime (deno) for challenge solving during extraction
///   2. Browser impersonation (curl_cffi) to avoid bot detection
///   3. Cookies (optional, only when deno is present for n-signature solving)
///
/// Without these, metadata + subtitles may still work but video downloads
/// fail with HTTP 403 Forbidden.
pub fn ytdlp_network_opts(use_cookies: bool) -> Vec<String> {
    let mut opts = Vec::new();
    let home = dirs::home_dir().unwrap_or_default();

    // JS runtime for YouTube challenge solving (required since mid-2025)
    let has_deno = which::which("deno").is_ok() || home.join(".deno/bin/deno").is_file();
    if has_deno {
        opts.extend(["--js-runtimes".into(), "deno".into()]);
    }

    // Browser impersonation via curl_cffi (bypasses bot detection)
    let has_curl_cffi = std::process::Command::new("python3")
        .args(["-c", "import curl_cffi"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if has_curl_cffi {
        opts.extend(["--impersonate".into(), "chrome".into()]);
    }

    // Chrome cookies for authenticated sessions (opt-in only — breaks android_vr)
    if use_cookies && has_chrome_cookies() {
        opts.extend(["--cookies-from-browser".into(), "chrome".into()]);
        opts.extend(["--extractor-args".into(), "youtube:player_client=web".into()]);
    }

    opts
}

// ---------------------------------------------------------------------------
// URL / local helpers
// ---------------------------------------------------------------------------

pub fn is_url(source: &str) -> bool {
    source.starts_with("http://") || source.starts_with("https://")
}

pub fn resolve_local(path: &str) -> Result<DownloadResult> {
    let p = Path::new(path)
        .canonicalize()
        .map_err(|e| WatchError::Download(format!("File not found or not accessible '{}': {}", path, e)))?;

    // Check for common video/audio file extensions
    let valid_extensions = [
        "mp4", "mkv", "webm", "mov", "avi", "m4v", "flv", "wmv",
        "ts", "mts", "3gp", "ogv",
        "mp3", "m4a", "wav", "flac", "ogg", "aac",
    ];
    if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
        if !valid_extensions.contains(&ext.to_lowercase().as_str()) {
            eprintln!(
                "[watch2] warning: '{}' has extension '.{}' which may not be a supported video/audio file",
                path, ext
            );
        }
    } else {
        eprintln!(
            "[watch2] warning: '{}' has no file extension — may not be a video file",
            path
        );
    }

    let title = p.file_name().unwrap_or_default().to_string_lossy().to_string();
    Ok(DownloadResult {
        video_path: Some(p.clone()),
        subtitle_path: None,
        info: VideoInfo {
            title: title.clone(),
            ..Default::default()
        },
        title,
        downloaded: false,
    })
}

// ---------------------------------------------------------------------------
// fetch_captions  — subtitles only (skip-download), with YouTube 2026 opts
// ---------------------------------------------------------------------------

/// Build the yt-dlp `--sub-langs` pattern for a given language code.
///
/// YouTube often uses "en" but auto-generated subs appear as "en.*" (e.g.
/// "en.auto", "en-orig"). We use glob patterns so both manual and auto subs
/// are matched.
fn subtitle_lang_pattern(lang: &str) -> String {
    if lang == "en" {
        "en.*".to_string()
    } else {
        format!("{}.*", lang)
    }
}

/// Run `yt-dlp --list-subs` and parse available manual/auto subtitle languages.
///
/// Returns `(manual: Vec<String>, auto: Vec<String>)` of language codes.
fn list_available_subtitles(url: &str, use_cookies: bool) -> (Vec<String>, Vec<String>) {
    let mut cmd = Command::new("yt-dlp");
    let mut args: Vec<&str> = vec!["--skip-download", "--list-subs", "--no-playlist"];

    // Apply network opts for YouTube reliability
    let network_opts = ytdlp_network_opts(use_cookies);
    for opt in &network_opts {
        args.push(opt.as_str());
    }
    args.push("--");
    args.push(url);

    let output = match cmd.args(&args).output() {
        Ok(o) => o,
        Err(_) => return (Vec::new(), Vec::new()),
    };
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut manual = Vec::new();
    let mut auto = Vec::new();
    let mut in_manual = false;
    let mut in_auto = false;

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.contains("Available manual subtitles") {
            in_manual = true;
            in_auto = false;
            continue;
        }
        if trimmed.contains("Available automatic") {
            in_auto = true;
            in_manual = false;
            continue;
        }
        // Empty line or new section ends the block
        if trimmed.is_empty() && (in_manual || in_auto) {
            in_manual = false;
            in_auto = false;
            continue;
        }
        // Parse lines like: "  en  English  (default)"
        if let Some(lang) = trimmed.split_whitespace().next() {
            let lang = lang.to_string();
            if in_manual && !manual.contains(&lang) {
                manual.push(lang);
            } else if in_auto && !auto.contains(&lang) {
                auto.push(lang);
            }
        }
    }

    (manual, auto)
}

pub fn fetch_captions(url: &str, out_dir: &Path, use_cookies: bool) -> Result<DownloadResult> {
    std::fs::create_dir_all(out_dir)?;
    let output_template = out_dir.join("video.%(ext)s").to_string_lossy().to_string();

    let network_opts = ytdlp_network_opts(use_cookies);

    // --- First pass: fetch metadata only (for language detection) ---
    let mut meta_args: Vec<&str> = Vec::new();
    for opt in &network_opts {
        meta_args.push(opt.as_str());
    }
    meta_args.extend([
        "--skip-download",
        "--write-info-json",
        "--no-playlist",
        "--ignore-errors",
        "-o", &output_template,
        "--", url,
    ]);
    let _ = Command::new("yt-dlp").args(&meta_args).status();

    let info = extract_info(out_dir);

    // --- Detect best subtitle language ---
    let (manual_subs, auto_subs) = list_available_subtitles(url, use_cookies);
    let detected_lang = suggest_subtitle_language(
        info.language.as_deref(),
        &manual_subs,
        &auto_subs,
    );
    let lang_pattern = subtitle_lang_pattern(&detected_lang);
    let lang_name = get_language_name(&detected_lang);
    eprintln!(
        "[watch2] subtitle language: {} ({}) — pattern: {}",
        lang_name, detected_lang, lang_pattern
    );

    // --- Second pass: fetch subtitles in detected language ---
    let mut args: Vec<&str> = Vec::new();
    for opt in &network_opts {
        args.push(opt.as_str());
    }
    args.extend([
        "--skip-download",
        "--write-info-json",
        "--write-subs",
        "--write-auto-subs",
        "--sub-langs", &lang_pattern,
        "--sub-format", "json3/best",
        "--no-playlist",
        "--ignore-errors",
        "--sleep-subtitles", "3",
        "-o", &output_template,
        "--", url,
    ]);

    let status = Command::new("yt-dlp")
        .args(&args)
        .status();

    match status {
        Ok(s) if s.success() => {
            let subtitle_path = find_subtitle(out_dir);
            let info = extract_info(out_dir);
            let title = info.title.clone();
            Ok(DownloadResult {
                video_path: None,
                subtitle_path,
                info,
                title,
                downloaded: false,
            })
        }
        Ok(_) => Err(WatchError::Download("yt-dlp caption fetch failed".into())),
        Err(e) => Err(WatchError::Download(format!("yt-dlp not found: {}", e))),
    }
}

// ---------------------------------------------------------------------------
// download_video — full download with subtitles, YouTube 2026 opts
// ---------------------------------------------------------------------------

pub fn download_video(url: &str, out_dir: &Path, use_cookies: bool) -> Result<DownloadResult> {
    std::fs::create_dir_all(out_dir)?;
    let output_template = out_dir.join("video.%(ext)s").to_string_lossy().to_string();

    let network_opts = ytdlp_network_opts(use_cookies);

    // --- First pass: fetch metadata + subtitles for language detection ---
    let mut meta_args: Vec<&str> = Vec::new();
    for opt in &network_opts {
        meta_args.push(opt.as_str());
    }
    meta_args.extend([
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
    ]);
    let _ = Command::new("yt-dlp").args(&meta_args).status();

    let info = extract_info(out_dir);

    // --- Detect best subtitle language ---
    let (manual_subs, auto_subs) = list_available_subtitles(url, use_cookies);
    let detected_lang = suggest_subtitle_language(
        info.language.as_deref(),
        &manual_subs,
        &auto_subs,
    );
    let lang_pattern = subtitle_lang_pattern(&detected_lang);
    let lang_name = get_language_name(&detected_lang);
    eprintln!(
        "[watch2] subtitle language: {} ({}) — pattern: {}",
        lang_name, detected_lang, lang_pattern
    );

    // --- Second pass: full download with detected subtitle language ---
    let mut args: Vec<&str> = Vec::new();
    for opt in &network_opts {
        args.push(opt.as_str());
    }
    args.extend([
        "--write-subs",
        "--write-auto-subs",
        "--sub-langs", &lang_pattern,
        "--sub-format", "json3/best",
        "--no-playlist",
        "--ignore-errors",
        "--sleep-subtitles", "3",
        "-o", &output_template,
        "--", url,
    ]);

    let status = Command::new("yt-dlp")
        .args(&args)
        .status();

    match status {
        Ok(s) if s.success() => {
            let video_path = find_video(out_dir);
            let subtitle_path = find_subtitle(out_dir);
            let info = extract_info(out_dir);
            let title = info.title.clone();
            Ok(DownloadResult {
                video_path,
                subtitle_path,
                info,
                title,
                downloaded: true,
            })
        }
        Ok(_) => Err(WatchError::Download("yt-dlp download failed".into())),
        Err(e) => Err(WatchError::Download(format!("yt-dlp not found: {}", e))),
    }
}

// ---------------------------------------------------------------------------
// VideoInfo extraction
// ---------------------------------------------------------------------------

/// Extract rich video metadata from the `video.info.json` sidecar written by
/// yt-dlp's `--write-info-json` flag.
pub fn extract_info(dir: &Path) -> VideoInfo {
    // Look for any *.info.json in the directory (video.info.json or <id>.info.json)
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "json")
            && path.to_string_lossy().contains("info")
        {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    let title = json["title"]
                        .as_str()
                        .unwrap_or("Unknown")
                        .to_string();
                    let uploader = json["uploader"]
                        .as_str()
                        .map(|s| s.to_string());
                    let duration = json["duration"]
                        .as_f64()
                        .or_else(|| json["duration"].as_i64().map(|i| i as f64));
                    let language = json["language"]
                        .as_str()
                        .or_else(|| json["language"].as_i64().map(|_| "en"))
                        .map(|s| s.to_string());
                    let description = json["description"].as_str().map(|s| {
                        if s.len() > 500 {
                            format!("{}…", &s[..500])
                        } else {
                            s.to_string()
                        }
                    });

                    return VideoInfo {
                        title,
                        uploader,
                        duration,
                        language,
                        description,
                    };
                }
            }
        }
    }
    VideoInfo::default()
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

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
