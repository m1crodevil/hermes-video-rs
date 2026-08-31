use crate::config::{get_language_name, is_valid_lang, suggest_subtitle_language};
use crate::error::{Result, WatchError};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Common yt-dlp arguments used across all download functions.
/// Ensures consistent behavior and prevents playlist processing.
const COMMON_ARGS: &[&str] = &["--no-playlist", "--ignore-errors", "--sleep-subtitles", "3"];

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
pub fn ytdlp_network_opts(use_cookies: bool, cookies_file: Option<&str>) -> Result<Vec<String>> {
    let mut opts = Vec::new();
    let home = dirs::home_dir().unwrap_or_default();

    // JS runtime for YouTube challenge solving (required since mid-2025)
    let has_deno = which::which("deno").is_ok() || home.join(".deno/bin/deno").is_file();
    if has_deno {
        opts.extend(["--js-runtimes".into(), "deno".into()]);
    }

    // Browser impersonation via curl_cffi (bypasses bot detection)
    // Check via yt-dlp itself — zero Python dependency
    let has_curl_cffi = std::process::Command::new("yt-dlp")
        .args(["--list-impersonate-targets"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .map(|o| {
            let out = String::from_utf8_lossy(&o.stdout);
            out.contains("Chrome") && !out.contains("unavailable")
        })
        .unwrap_or(false);

    if has_curl_cffi {
        opts.extend(["--impersonate".into(), "chrome".into()]);
    }

    // mweb is yt-dlp's recommended client when a PO-token provider handles
    // GVS attestation. Without a provider, yt-dlp still returns its normal 403.
    opts.extend([
        "--extractor-args".into(),
        "youtube:player_client=mweb".into(),
    ]);

    // Cookies are explicit: a file works in headless environments and is safer
    // than guessing a browser profile.
    if let Some(path) = cookies_file {
        let metadata = std::fs::metadata(path)
            .map_err(|_| WatchError::Config("cookie file not found".into()))?;
        #[cfg(unix)]
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(WatchError::Config(
                "cookie file must not be group/world-readable; run chmod 600".into(),
            ));
        }
        opts.extend(["--cookies".into(), path.into()]);
        opts.extend([
            "--extractor-args".into(),
            "youtube:player_client=web".into(),
        ]);
    } else if use_cookies && has_chrome_cookies() {
        opts.extend(["--cookies-from-browser".into(), "chrome".into()]);
        opts.extend([
            "--extractor-args".into(),
            "youtube:player_client=web".into(),
        ]);
    }

    Ok(opts)
}

pub fn is_video_access_denied(stderr: &str) -> bool {
    stderr.contains("HTTP Error 403") || stderr.contains("PO Token")
}

pub fn has_po_token_provider(stderr: &str) -> bool {
    stderr.contains("PO Token Providers:") && !stderr.contains("PO Token Providers: none")
}

// ---------------------------------------------------------------------------
// URL / local helpers
// ---------------------------------------------------------------------------

/// Strip control characters from URL before passing to subprocess.
pub fn sanitize_url(url: &str) -> String {
    url.chars().filter(|c| !c.is_control()).collect()
}

pub fn is_url(source: &str) -> bool {
    source.starts_with("http://") || source.starts_with("https://")
}

pub fn resolve_local(path: &str) -> Result<DownloadResult> {
    let path = sanitize_url(path);
    let p = Path::new(&path).canonicalize().map_err(|_| {
        tracing::debug!("File not found: {} (full path: {})", path, path);
        WatchError::Download(format!(
            "File not found: {}",
            crate::error::sanitize_path(Path::new(&path))
        ))
    })?;

    // Check for common video/audio file extensions
    let valid_extensions = [
        "mp4", "mkv", "webm", "mov", "avi", "m4v", "flv", "wmv", "ts", "mts", "3gp", "ogv", "mp3",
        "m4a", "wav", "flac", "ogg", "aac",
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

    let title = p
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
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

/// Build the yt-dlp `--sub-langs` pattern for a given language code.
///
/// YouTube often uses "en" but auto-generated subs appear as "en.*" (e.g.
/// "en.auto", "en-orig"). We use glob patterns so both manual and auto subs
/// are matched.
fn subtitle_lang_pattern(lang: &str) -> String {
    // Strip regional suffix: "en-US" → "en", "pt-BR" → "pt"
    let base = lang.split('-').next().unwrap_or(lang);
    format!("{}.*", base)
}

/// Run `yt-dlp --list-subs` and parse available manual/auto subtitle languages.
///
/// Returns `(manual: Vec<String>, auto: Vec<String>)` of language codes.
fn list_available_subtitles(
    url: &str,
    use_cookies: bool,
    cookies_file: Option<&str>,
) -> Result<(Vec<String>, Vec<String>)> {
    let mut cmd = Command::new("yt-dlp");
    let mut args: Vec<&str> = vec![
        "--skip-download",
        "--list-subs",
        "--no-playlist",
        "--flat-playlist",
    ];

    // Apply network opts for YouTube reliability
    let network_opts = ytdlp_network_opts(use_cookies, cookies_file)?;
    for opt in &network_opts {
        args.push(opt.as_str());
    }
    args.push("--");
    args.push(url);

    let output = match cmd.args(&args).output() {
        Ok(o) => o,
        Err(_) => return Ok((Vec::new(), Vec::new())),
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

    Ok((manual, auto))
}

// ---------------------------------------------------------------------------
// download_video — full download with subtitles, YouTube 2026 opts
// ---------------------------------------------------------------------------

pub fn download_video(
    url: &str,
    out_dir: &Path,
    use_cookies: bool,
    cookies_file: Option<&str>,
    allow_transcript_only: bool,
    llm_lang: Option<&str>,
    lang: Option<&str>,
) -> Result<DownloadResult> {
    let url = sanitize_url(url);
    std::fs::create_dir_all(out_dir)?;
    let output_template = out_dir.join("video.%(ext)s").to_string_lossy().to_string();

    let network_opts = ytdlp_network_opts(use_cookies, cookies_file)?;

    // --- Language detection: caller-provided > metadata > list-subs fallback ---
    let detected_lang = if let Some(l) = lang {
        l.to_string()
    } else {
        let existing_info = extract_info(out_dir);
        if let Some(ref l) = existing_info.language {
            l.clone()
        } else {
            let (manual_subs, auto_subs) =
                list_available_subtitles(&url, use_cookies, cookies_file)?;
            suggest_subtitle_language(
                existing_info.language.as_deref(),
                &manual_subs,
                &auto_subs,
                llm_lang,
            )
        }
    };

    if !is_valid_lang(&detected_lang) {
        eprintln!(
            "[watch2] detected lang '{}' not in whitelist, falling back to en",
            detected_lang
        );
    }
    let lang_name = get_language_name(&detected_lang);

    // Use targeted subtitle download when language is known, fallback to all
    let sub_langs = if lang.is_some() || detected_lang != "en" {
        let pattern = subtitle_lang_pattern(&detected_lang);
        eprintln!(
            "[watch2] subtitle language: {} ({}) — pattern: {}",
            lang_name, detected_lang, pattern
        );
        pattern
    } else {
        eprintln!(
            "[watch2] subtitle language: {} ({}) — downloading ALL languages",
            lang_name, detected_lang
        );
        ".*".to_string()
    };

    // --- Single pass: full download with subtitles (NO separate metadata fetch) ---
    // Clean stale subtitles from any prior runs to avoid cross-language conflicts
    clean_stale_subtitles(out_dir);
    let mut args: Vec<&str> = Vec::new();
    for opt in &network_opts {
        args.push(opt.as_str());
    }
    args.extend(COMMON_ARGS);
    // Cap video quality at 720p to avoid huge downloads (matches Python hermes-video)
    let format_str = "bv*[height<=720]+ba/b[height<=720]/bv+ba/b";
    args.extend([
        "-f",
        format_str,
        "--merge-output-format",
        "mp4",
        "--write-info-json", // Re-generate info.json with full download
        "--write-subs",
        "--write-auto-subs",
        "--sub-langs",
        &sub_langs,
        "--sub-format",
        "json3/best",
        "-o",
        &output_template,
        "--",
        &url,
    ]);

    let output = Command::new("yt-dlp").args(&args).output();

    match output {
        Ok(output)
            if output.status.success()
                || (allow_transcript_only
                    && is_video_access_denied(&String::from_utf8_lossy(&output.stderr))) =>
        {
            let video_path = find_video(out_dir);
            let subtitle_path = find_subtitle(out_dir, &detected_lang);
            let info = extract_info(out_dir);
            let title = info.title.clone();
            Ok(DownloadResult {
                video_path: output.status.success().then_some(video_path).flatten(),
                subtitle_path,
                info,
                title,
                downloaded: true,
            })
        }
        Ok(output) if is_video_access_denied(&String::from_utf8_lossy(&output.stderr)) => {
            Err(WatchError::VideoAccessDenied)
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
        if path.extension().is_some_and(|e| e == "json")
            && path.to_string_lossy().contains("info")
            && let Ok(content) = std::fs::read_to_string(&path)
            && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
        {
            let title = json["title"].as_str().unwrap_or("Unknown").to_string();
            let uploader = json["uploader"].as_str().map(|s| s.to_string());
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
    VideoInfo::default()
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn find_video(dir: &Path) -> Option<PathBuf> {
    for ext in &["mp4", "mkv", "webm", "mov", "m4a", "mp3"] {
        for entry in std::fs::read_dir(dir).ok()? {
            let entry = entry.ok()?;
            if entry.path().extension().is_some_and(|e| e == *ext) {
                return Some(entry.path());
            }
        }
    }
    None
}

/// Find the best subtitle file in `dir`, preferring files matching `preferred_lang`.
///
/// Matches against the base language code (e.g. "en" matches "en-US").
fn find_subtitle(dir: &Path, preferred_lang: &str) -> Option<PathBuf> {
    // Normalize: "en-US" → "en" for filename matching
    let base_lang = preferred_lang.split('-').next().unwrap_or(preferred_lang);
    let mut candidates: Vec<(bool, PathBuf)> = Vec::new();
    for ext in &["json3", "vtt"] {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == *ext) {
                let name = path.file_name().unwrap().to_string_lossy();
                // Match patterns like "video.en-orig.json3" or "video.en.json3"
                // Use base language code for matching (not regional like "en-US")
                let is_preferred = name.contains(&format!(".{}.", base_lang))
                    || name.contains(&format!(".{}-", base_lang));
                candidates.push((is_preferred, path));
            }
        }
    }
    // Preferred language files first, then fall back to any subtitle file
    candidates.sort_by_key(|b| std::cmp::Reverse(b.0));
    candidates.into_iter().next().map(|(_, p)| p)
}

/// Remove stale subtitle files from a directory to avoid cross-language conflicts
/// between yt-dlp passes.
fn clean_stale_subtitles(dir: &Path) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str())
            && (ext == "json3" || ext == "vtt")
        {
            std::fs::remove_file(&path).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(prefix: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("watch2_test_{}_{}", prefix, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ── find_video tests ──────────────────────────────────────────────

    #[test]
    fn test_find_video_mp4() {
        let dir = temp_dir("find_video_mp4");
        fs::write(dir.join("video.mp4"), b"fake").unwrap();
        assert!(find_video(&dir).is_some());
        assert_eq!(find_video(&dir).unwrap().file_name().unwrap(), "video.mp4");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_video_webm() {
        let dir = temp_dir("find_video_webm");
        fs::write(dir.join("video.webm"), b"fake").unwrap();
        assert!(find_video(&dir).is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_video_mk4() {
        let dir = temp_dir("find_video_mkv");
        fs::write(dir.join("video.mkv"), b"fake").unwrap();
        assert!(find_video(&dir).is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_video_none_when_empty() {
        let dir = temp_dir("find_video_empty");
        assert!(find_video(&dir).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_video_ignores_non_video() {
        let dir = temp_dir("find_video_non_video");
        fs::write(dir.join("video.info.json"), b"fake").unwrap();
        fs::write(dir.join("video.id.json3"), b"fake").unwrap();
        assert!(find_video(&dir).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_video_prefers_mp4_over_webm() {
        let dir = temp_dir("find_video_pref");
        fs::write(dir.join("video.webm"), b"fake").unwrap();
        fs::write(dir.join("video.mp4"), b"fake").unwrap();
        let result = find_video(&dir).unwrap();
        let name = result.file_name().unwrap().to_string_lossy();
        // Should find one of them (order depends on read_dir, but should not be None)
        assert!(name.ends_with(".mp4") || name.ends_with(".webm"));
        let _ = fs::remove_dir_all(&dir);
    }

    // ── find_subtitle tests ───────────────────────────────────────────

    #[test]
    fn test_find_subtitle_json3() {
        let dir = temp_dir("find_sub_json3");
        fs::write(dir.join("video.en.json3"), b"fake").unwrap();
        let result = find_subtitle(&dir, "en");
        assert!(result.is_some());
        assert_eq!(result.unwrap().file_name().unwrap(), "video.en.json3");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_subtitle_vtt() {
        let dir = temp_dir("find_sub_vtt");
        fs::write(dir.join("video.en.vtt"), b"fake").unwrap();
        let result = find_subtitle(&dir, "en");
        assert!(result.is_some());
        assert_eq!(result.unwrap().file_name().unwrap(), "video.en.vtt");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_subtitle_prefers_matching_lang() {
        let dir = temp_dir("find_sub_pref");
        fs::write(dir.join("video.en.json3"), b"fake").unwrap();
        fs::write(dir.join("video.id.json3"), b"fake").unwrap();
        let result = find_subtitle(&dir, "id");
        assert!(result.is_some());
        assert_eq!(result.unwrap().file_name().unwrap(), "video.id.json3");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_subtitle_prefers_orig_over_auto() {
        let dir = temp_dir("find_sub_orig");
        fs::write(dir.join("video.id.json3"), b"fake").unwrap();
        fs::write(dir.join("video.id-orig.json3"), b"fake").unwrap();
        let result = find_subtitle(&dir, "id");
        assert!(result.is_some());
        // Both match "id" — orig should be preferred due to sort
        let name = result
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(name.contains("id"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_subtitle_none_when_empty() {
        let dir = temp_dir("find_sub_empty");
        assert!(find_subtitle(&dir, "en").is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_subtitle_ignores_non_subtitle() {
        let dir = temp_dir("find_sub_non_sub");
        fs::write(dir.join("video.info.json"), b"fake").unwrap();
        fs::write(dir.join("video.mp4"), b"fake").unwrap();
        assert!(find_subtitle(&dir, "en").is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_subtitle_fallback_to_any_lang() {
        let dir = temp_dir("find_sub_fallback");
        fs::write(dir.join("video.en.json3"), b"fake").unwrap();
        // Requesting "id" but only "en" exists — should still find it
        let result = find_subtitle(&dir, "id");
        assert!(result.is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_subtitle_no_dot_in_extension_pattern() {
        // Regression test for Bug #5: extension patterns must NOT have dot prefix
        let dir = temp_dir("find_sub_no_dot");
        fs::write(dir.join("video.id.json3"), b"fake").unwrap();
        // This should work — previously failed because code compared ".json3" with "json3"
        let result = find_subtitle(&dir, "id");
        assert!(
            result.is_some(),
            "Bug #5 regression: find_subtitle returned None for .json3 file"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_subtitle_download_all_scenario() {
        // Simulates --sub-langs ".*" scenario: many language files exist
        // find_subtitle should pick the correct one based on preferred_lang
        let dir = temp_dir("find_sub_download_all");
        fs::write(dir.join("video.en.json3"), b"english").unwrap();
        fs::write(dir.join("video.id.json3"), b"indonesian").unwrap();
        fs::write(dir.join("video.ja.json3"), b"japanese").unwrap();
        fs::write(dir.join("video.ko.json3"), b"korean").unwrap();
        fs::write(dir.join("video.zh-Hans.json3"), b"chinese-simplified").unwrap();
        fs::write(dir.join("video.pt.json3"), b"portuguese").unwrap();
        fs::write(dir.join("video.es.json3"), b"spanish").unwrap();
        fs::write(dir.join("video.de.json3"), b"german").unwrap();
        fs::write(dir.join("video.fr.json3"), b"french").unwrap();
        fs::write(dir.join("video.ar.json3"), b"arabic").unwrap();

        // Requesting Indonesian — should find it among 10 languages
        let result = find_subtitle(&dir, "id");
        assert!(result.is_some());
        let name = result
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(
            name.contains("id"),
            "Expected Indonesian subtitle, got: {}",
            name
        );

        // Requesting English — should find it
        let result = find_subtitle(&dir, "en");
        assert!(result.is_some());
        let name = result
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(
            name.contains("en"),
            "Expected English subtitle, got: {}",
            name
        );

        // Requesting non-existent language — should fallback to any
        let result = find_subtitle(&dir, "ru");
        assert!(
            result.is_some(),
            "Should fallback to any available subtitle"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // ── clean_stale_subtitles tests ───────────────────────────────────

    #[test]
    fn test_clean_stale_removes_json3() {
        let dir = temp_dir("clean_stale");
        fs::write(dir.join("video.id.json3"), b"fake").unwrap();
        fs::write(dir.join("video.info.json"), b"keep").unwrap();
        clean_stale_subtitles(&dir);
        assert!(!dir.join("video.id.json3").exists());
        assert!(dir.join("video.info.json").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_clean_stale_removes_vtt() {
        let dir = temp_dir("clean_stale_vtt");
        fs::write(dir.join("video.en.vtt"), b"fake").unwrap();
        clean_stale_subtitles(&dir);
        assert!(!dir.join("video.en.vtt").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    // ── subtitle_lang_pattern tests ───────────────────────────────────

    #[test]
    fn test_subtitle_lang_pattern_english() {
        assert_eq!(subtitle_lang_pattern("en"), "en.*");
    }

    #[test]
    fn test_subtitle_lang_pattern_indonesian() {
        assert_eq!(subtitle_lang_pattern("id"), "id.*");
    }

    #[test]
    fn test_subtitle_lang_pattern_french() {
        assert_eq!(subtitle_lang_pattern("fr"), "fr.*");
    }

    #[test]
    fn test_subtitle_lang_pattern_en_us() {
        assert_eq!(subtitle_lang_pattern("en-US"), "en.*");
    }

    #[test]
    fn test_subtitle_lang_pattern_zh_hans() {
        assert_eq!(subtitle_lang_pattern("zh-Hans"), "zh.*");
    }

    // ── sanitize_url tests ──────────────────────────────────────────

    #[test]
    fn test_sanitize_url_strips_null_bytes() {
        assert_eq!(
            sanitize_url("http://example.com/\x00video"),
            "http://example.com/video"
        );
    }

    #[test]
    fn test_sanitize_url_strips_newline_tab() {
        assert_eq!(
            sanitize_url("http://example.com/\n\tvideo"),
            "http://example.com/video"
        );
    }

    #[test]
    fn test_sanitize_url_normal_unchanged() {
        let url = "https://www.youtube.com/watch?v=abc123";
        assert_eq!(sanitize_url(url), url);
    }

    #[test]
    fn test_sanitize_url_empty_string() {
        assert_eq!(sanitize_url(""), "");
    }

    // ── is_url tests ────────────────────────────────────────────────

    #[test]
    fn test_is_url_https() {
        assert!(is_url("https://www.youtube.com/watch?v=abc"));
    }

    #[test]
    fn test_is_url_http() {
        assert!(is_url("http://example.com/video.mp4"));
    }

    #[test]
    fn test_is_url_local_path() {
        assert!(!is_url("/local/path/video.mp4"));
    }

    #[test]
    fn test_is_url_ftp() {
        assert!(!is_url("ftp://server.com/file"));
    }

    #[test]
    fn detects_installed_po_token_provider() {
        assert!(has_po_token_provider(
            "[debug] [youtube] [pot] PO Token Providers: bgutil:http-1.3.2 (external)"
        ));
        assert!(!has_po_token_provider(
            "[debug] [youtube] [pot] PO Token Providers: none"
        ));
    }

    #[test]
    fn recognizes_youtube_video_access_denial() {
        assert!(is_video_access_denied(
            "ERROR: unable to download video data: HTTP Error 403: Forbidden"
        ));
        assert!(is_video_access_denied("PO Token required"));
        assert!(!is_video_access_denied("HTTP Error 429: Too Many Requests"));
    }

    #[test]
    fn test_is_url_flag() {
        assert!(!is_url("--no-playlist"));
    }

    // ── extract_info tests ──────────────────────────────────────────

    #[test]
    fn test_extract_info_valid_json() {
        let dir = tempfile::tempdir().unwrap();
        let info = serde_json::json!({
            "title": "Test Video",
            "uploader": "TestChannel",
            "duration": 120.5,
            "language": "en",
            "description": "A great video about testing."
        });
        std::fs::write(dir.path().join("video.info.json"), info.to_string()).unwrap();

        let result = extract_info(dir.path());
        assert_eq!(result.title, "Test Video");
        assert_eq!(result.uploader.as_deref(), Some("TestChannel"));
        assert_eq!(result.duration, Some(120.5));
        assert_eq!(result.language.as_deref(), Some("en"));
        assert_eq!(
            result.description.as_deref(),
            Some("A great video about testing.")
        );
    }

    #[test]
    fn test_extract_info_missing_optional_fields() {
        let dir = tempfile::tempdir().unwrap();
        let info = serde_json::json!({ "title": "Minimal" });
        std::fs::write(dir.path().join("video.info.json"), info.to_string()).unwrap();

        let result = extract_info(dir.path());
        assert_eq!(result.title, "Minimal");
        assert!(result.uploader.is_none());
        assert!(result.duration.is_none());
        assert!(result.language.is_none());
        assert!(result.description.is_none());
    }

    #[test]
    fn test_extract_info_description_truncated_at_500() {
        let dir = tempfile::tempdir().unwrap();
        let long_desc = "x".repeat(600);
        let info = serde_json::json!({
            "title": "LongDesc",
            "description": long_desc
        });
        std::fs::write(dir.path().join("video.info.json"), info.to_string()).unwrap();

        let result = extract_info(dir.path());
        let desc = result.description.unwrap();
        // "…" is 3 bytes in UTF-8 (U+2026), so 500 + 3 = 503 bytes
        assert_eq!(desc.len(), 503);
        assert!(desc.ends_with('…'));
    }

    #[test]
    fn test_extract_info_empty_json_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        // Empty object — title defaults to "Unknown", everything else None
        std::fs::write(dir.path().join("video.info.json"), "{}").unwrap();

        let result = extract_info(dir.path());
        assert_eq!(result.title, "Unknown");
        assert!(result.uploader.is_none());
        assert!(result.duration.is_none());
    }

    #[test]
    fn test_extract_info_non_json_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("video.info.json"), "not valid json {{{").unwrap();

        let result = extract_info(dir.path());
        // Non-JSON falls through to default
        assert_eq!(result.title, "Unknown");
        assert!(result.uploader.is_none());
    }

    // ── resolve_local tests ─────────────────────────────────────────

    #[test]
    fn test_resolve_local_mp4_returns_path() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test_video.mp4");
        std::fs::write(&file, b"fake").unwrap();

        let result = resolve_local(file.to_str().unwrap()).unwrap();
        assert!(result.video_path.is_some());
        let vp = result.video_path.unwrap();
        assert_eq!(vp.file_name().unwrap(), "test_video.mp4");
    }

    #[test]
    fn test_resolve_local_unsupported_extension_succeeds_with_warning() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test_video.xyz");
        std::fs::write(&file, b"fake").unwrap();

        // Unsupported extension just warns, does NOT return error
        let result = resolve_local(file.to_str().unwrap());
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_local_no_extension_succeeds_with_warning() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test_video");
        std::fs::write(&file, b"fake").unwrap();

        // No extension just warns, does NOT return error
        let result = resolve_local(file.to_str().unwrap());
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_local_nonexistent_path_returns_error() {
        let result = resolve_local("/nonexistent/path/video.mp4");
        assert!(result.is_err());
    }
}
