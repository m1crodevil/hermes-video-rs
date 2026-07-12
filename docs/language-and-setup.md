# Language Detection & YouTube 2026 Setup Reference

> Subtitle language detection, yt-dlp flags, and auto-installer patterns.

## 1. Subtitle Language Detection

### Language Code Map (26 languages)

```rust
pub const LANGUAGE_NAMES: &[(&str, &str)] = &[
    ("id", "Indonesian"), ("en", "English"), ("ms", "Malay"),
    ("jv", "Javanese"), ("su", "Sundanese"), ("ar", "Arabic"),
    ("zh", "Chinese"), ("ja", "Japanese"), ("ko", "Korean"),
    ("es", "Spanish"), ("pt", "Portuguese"), ("fr", "French"),
    ("de", "German"), ("it", "Italian"), ("ru", "Russian"),
    ("hi", "Hindi"), ("th", "Thai"), ("vi", "Vietnamese"),
    ("tl", "Filipino"), ("tr", "Turkish"), ("pl", "Polish"),
    ("nl", "Dutch"), ("sv", "Swedish"), ("da", "Danish"),
    ("no", "Norwegian"), ("fi", "Finnish"),
];

pub fn get_language_name(code: &str) -> &str {
    LANGUAGE_NAMES.iter()
        .find(|(c, _)| *c == code)
        .map(|(_, name)| *name)
        .unwrap_or("Unknown")
}
```

### Language Detection Algorithm (from Python)

```rust
pub fn suggest_subtitle_language(
    video_language: Option<&str>,   // From info.json "language" field
    available_manual: &[String],    // From yt-dlp --list-subs
    available_auto: &[String],      // From yt-dlp --list-subs
) -> String {
    let vid_lang = video_language.unwrap_or("en");
    
    // Priority 1: Manual subs in video language
    if available_manual.iter().any(|l| l == vid_lang) {
        return vid_lang.to_string();
    }
    
    // Priority 2: Auto subs in video language
    if available_auto.iter().any(|l| l == vid_lang) {
        return vid_lang.to_string();
    }
    
    // Priority 3: Manual English
    if available_manual.iter().any(|l| l == "en") {
        return "en".to_string();
    }
    
    // Priority 4: Auto English
    if available_auto.iter().any(|l| l == "en") {
        return "en".to_string();
    }
    
    // Priority 5: Video language (even if not in available)
    vid_lang.to_string()
}
```

### Subtitle Language Pattern for yt-dlp

```rust
// Build language pattern for yt-dlp --sub-langs
fn subtitle_lang_pattern(best_lang: &str) -> String {
    if best_lang == "en" {
        "en.*".to_string()
    } else {
        format!("{}.*", best_lang)
    }
}
```

### Parse yt-dlp --list-subs output

```rust
fn parse_available_subtitles(output: &str) -> (Vec<String>, Vec<String>) {
    let mut manual = Vec::new();
    let mut auto = Vec::new();
    let mut in_auto = false;
    
    for line in output.lines() {
        let line = line.trim();
        if line.contains("Available automatic") {
            in_auto = true;
            continue;
        }
        if line.contains("Available manual") || line.contains("Available subtitles") {
            in_auto = false;
            continue;
        }
        
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 1 && parts[0].len() >= 2 && parts[0].chars().all(|c| c.is_alphabetic()) {
            let lang_code = parts[0].to_string();
            if in_auto {
                auto.push(lang_code);
            } else {
                manual.push(lang_code);
            }
        }
    }
    
    (manual, auto)
}
```

---

## 2. yt-dlp YouTube 2026 CLI Flags

### Required for YouTube downloads (since mid-2025)

| Flag | Purpose | When Needed |
|------|---------|-------------|
| `--js-runtimes deno` | JS runtime for YouTube challenge solving | Always for YouTube |
| `--impersonate chrome` | Browser impersonation (bypasses bot detection) | When video download fails with 403 |
| `--cookies-from-browser chrome` | Use Chrome's authenticated cookies | For age-restricted/private/member videos |
| `--sleep-subtitles 3` | Sleep 3s between subtitle requests | Prevents HTTP 429 rate limiting |
| `--sub-format json3/best` | Prefer JSON3 subtitle format | Always |
| `--write-info-json` | Write video metadata to .info.json | Always (for title, language, duration) |
| `--no-playlist` | Don't download playlist | Always |
| `--ignore-errors` | Continue on subtitle errors | Always |

### Auto-detection of dependencies

```rust
use std::path::PathBuf;

fn has_deno() -> bool {
    which::which("deno").is_ok() 
        || dirs::home_dir()
            .map(|h| h.join(".deno/bin/deno").is_file())
            .unwrap_or(false)
}

fn has_curl_cffi() -> bool {
    // Check if yt-dlp has curl-cffi plugin installed
    // This is a yt-dlp Python plugin, not directly detectable from Rust
    // Best approach: try --impersonate and check if it works
    // Or check if the Python package is importable:
    std::process::Command::new("python3")
        .args(["-c", "import curl_cffi"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn has_chrome_cookies() -> bool {
    let home = dirs::home_dir().unwrap_or_default();
    [
        home.join(".config/google-chrome/Default/Cookies"),
        home.join(".config/chromium/Default/Cookies"),
        home.join("Library/Application Support/Google/Chrome/Default/Cookies"),
    ].iter().any(|p| p.exists())
}

fn ytdlp_network_opts() -> Vec<String> {
    let mut opts = Vec::new();
    
    if has_deno() {
        opts.extend(["--js-runtimes".into(), "deno".into()]);
    }
    if has_curl_cffi() && has_deno() {
        opts.extend(["--impersonate".into(), "chrome".into()]);
    }
    if has_deno() && has_chrome_cookies() {
        opts.extend(["--cookies-from-browser".into(), "chrome".into()]);
    }
    
    opts
}
```

### Combined YouTube 2026 command

```bash
yt-dlp \
  --js-runtimes deno \
  --impersonate chrome \
  --cookies-from-browser chrome \
  --sleep-subtitles 3 \
  --sub-format "json3/best" \
  --write-info-json \
  --write-subs \
  --write-auto-subs \
  --sub-langs "en.*" \
  --no-playlist \
  --ignore-errors \
  -N 4 \
  -f "bv*[height<=720]+ba/b[height<=720]/bv+ba/b" \
  --merge-output-format mp4 \
  -o "video.%(ext)s" \
  -- "URL"
```

---

## 3. Auto-Installer Patterns

### Detection
```rust
fn check_missing_binaries() -> Vec<String> {
    ["ffmpeg", "ffprobe", "yt-dlp"]
        .iter()
        .filter(|b| which::which(b).is_err())
        .map(|b| b.to_string())
        .collect()
}
```

### Linux installation

```rust
fn install_ffmpeg_linux() -> Result<()> {
    // Try apt first (Debian/Ubuntu)
    if which::which("apt").is_ok() {
        let status = std::process::Command::new("sudo")
            .args(["apt", "install", "-y", "ffmpeg"])
            .status()?;
        if status.success() { return Ok(()); }
    }
    
    // Try dnf (Fedora)
    if which::which("dnf").is_ok() {
        let status = std::process::Command::new("sudo")
            .args(["dnf", "install", "-y", "ffmpeg"])
            .status()?;
        if status.success() { return Ok(()); }
    }
    
    // Try pacman (Arch)
    if which::which("pacman").is_ok() {
        let status = std::process::Command::new("sudo")
            .args(["pacman", "-S", "--noconfirm", "ffmpeg"])
            .status()?;
        if status.success() { return Ok(()); }
    }
    
    Err("Could not install ffmpeg. Please install manually.".into())
}

fn install_ytdlp_linux() -> Result<()> {
    let local_bin = dirs::home_dir()
        .ok_or("Cannot find home dir")?
        .join(".local/bin");
    std::fs::create_dir_all(&local_bin)?;
    
    let ytdlp_path = local_bin.join("yt-dlp");
    
    // Download standalone binary
    let status = std::process::Command::new("curl")
        .args([
            "-L", "-o", ytdlp_path.to_str().unwrap(),
            "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp",
        ])
        .status()?;
    
    if status.success() {
        // Make executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&ytdlp_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&ytdlp_path, perms)?;
        }
        Ok(())
    } else {
        Err("Failed to download yt-dlp".into())
    }
}

fn install_deno() -> Result<()> {
    let deno_path = dirs::home_dir()
        .ok_or("Cannot find home dir")?
        .join(".deno/bin/deno");
    
    if deno_path.is_file() {
        return Ok(());
    }
    
    // Download install script
    let script_path = std::env::temp_dir().join("deno_install.sh");
    std::process::Command::new("curl")
        .args(["-fsSL", "-o", script_path.to_str().unwrap(),
               "https://deno.land/install.sh"])
        .status()?;
    
    // Execute install script
    std::process::Command::new("sh")
        .arg(script_path.to_str().unwrap())
        .status()?;
    
    // Cleanup
    std::fs::remove_file(&script_path).ok();
    
    if deno_path.is_file() {
        Ok(())
    } else {
        Err("Deno installation failed".into())
    }
}
```

### macOS installation (Homebrew)
```rust
fn install_macos(missing: &[String]) -> Result<()> {
    if which::which("brew").is_err() {
        return Err("Homebrew not installed. Install from https://brew.sh".into());
    }
    
    let mut packages = Vec::new();
    for bin in missing {
        match bin.as_str() {
            "ffmpeg" | "ffprobe" => {
                if !packages.contains(&"ffmpeg".to_string()) {
                    packages.push("ffmpeg".to_string());
                }
            }
            "yt-dlp" => packages.push("yt-dlp".to_string()),
            _ => {}
        }
    }
    
    if packages.is_empty() { return Ok(()); }
    
    let status = std::process::Command::new("brew")
        .arg("install")
        .args(&packages)
        .status()?;
    
    if status.success() { Ok(()) } else { Err("brew install failed".into()) }
}
```

### yt-dlp config file
```rust
fn ensure_ytdlp_config() -> Result<()> {
    let config_dir = dirs::home_dir()
        .ok_or("Cannot find home dir")?
        .join(".config/yt-dlp");
    std::fs::create_dir_all(&config_dir)?;
    
    let config_file = config_dir.join("config");
    if config_file.exists() { return Ok(()); }
    
    std::fs::write(&config_file, 
        "--impersonate chrome\n--js-runtimes deno\n")?;
    
    Ok(())
}
```

---

## 4. Video Cleanup Pattern

### RAII with tempfile (recommended)
```rust
use tempfile::TempDir;

fn main() -> Result<()> {
    let work = match out_dir {
        Some(dir) => {
            std::fs::create_dir_all(&dir)?;
            None  // User-specified dir, don't auto-delete
        }
        None => Some(TempDir::new()?),  // Auto-cleanup on drop
    };
    
    let work_path = work.as_ref()
        .map(|t| t.path().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(out_dir.as_ref().unwrap()));
    
    // ... processing ...
    
    // Cleanup downloaded video
    if !keep_video {
        if let Some(ref vp) = video_path {
            if downloaded {
                let size_mb = std::fs::metadata(vp)
                    .map(|m| m.len() / (1024 * 1024))
                    .unwrap_or(0);
                std::fs::remove_file(vp).ok();
                eprintln!("[watch2] cleaned up video ({} MB)", size_mb);
            }
        }
    }
    
    // Cleanup audio temp files
    let audio_tmp = work_path.join("audio.mp3");
    if audio_tmp.exists() {
        std::fs::remove_file(&audio_tmp).ok();
    }
    let chunks_dir = work_path.join("audio/chunks");
    if chunks_dir.exists() {
        std::fs::remove_dir_all(&chunks_dir).ok();
    }
    
    // TempDir auto-drops here if Some
    
    Ok(())
}
```

### Manual cleanup with --keep-video
```rust
// When user passes --keep-video, use into_path() to prevent auto-delete
let work = if keep_video || out_dir.is_some() {
    None  // Don't create TempDir
} else {
    Some(TempDir::new()?)
};
```
