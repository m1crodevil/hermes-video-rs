use std::path::{Path, PathBuf};
use which::which;

use std::os::unix::fs::PermissionsExt;

/// Check .env file permissions and warn if too permissive.
pub fn check_env_permissions(env_file: &std::path::Path) {
    if let Ok(meta) = std::fs::metadata(env_file) {
        let mode = meta.permissions().mode();
        if mode & 0o004 != 0 {
            eprintln!(
                "⚠️  {} is world-readable! Run: chmod 600 {}",
                env_file.display(),
                env_file.display()
            );
        } else if mode & 0o040 != 0 {
            eprintln!(
                "⚠️  {} is group-readable. Run: chmod 600 {}",
                env_file.display(),
                env_file.display()
            );
        }
    }
}

pub struct SetupStatus {
    pub can_proceed: bool,
    pub first_run: bool,
    pub missing_binaries: Vec<String>,
    pub has_api_key: bool,
    pub config_file: PathBuf,
}

pub fn check() -> SetupStatus {
    let missing = check_binaries();
    let home = dirs::home_dir().unwrap_or_default();
    let config_dir = home.join(".config").join("watch");
    let env_file = config_dir.join(".env");
    check_env_permissions(&env_file);
    let has_key = check_api_key(&env_file);
    let setup_complete = std::fs::read_to_string(&env_file)
        .map(|c| c.contains("SETUP_COMPLETE=true"))
        .unwrap_or(false);
    SetupStatus {
        can_proceed: missing.is_empty() && has_key,
        first_run: !setup_complete,
        missing_binaries: missing,
        has_api_key: has_key,
        config_file: env_file,
    }
}

fn check_binaries() -> Vec<String> {
    let mut missing = Vec::new();
    for bin in &["ffmpeg", "ffprobe", "yt-dlp"] {
        if which(bin).is_err() {
            missing.push(bin.to_string());
        }
    }
    missing
}

fn check_api_key(env_file: &Path) -> bool {
    if let Ok(content) = std::fs::read_to_string(env_file) {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || !line.contains('=') { continue; }
            let key = line.split('=').next().unwrap_or("").trim();
            let val = line.splitn(2, '=').nth(1).unwrap_or("").trim().trim_matches('"').trim_matches('\'');
            if (key == "GROQ_API_KEY" || key == "OPENAI_API_KEY") && !val.is_empty() {
                return true;
            }
        }
    }
    false
}
