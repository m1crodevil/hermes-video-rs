use std::path::PathBuf;

/// Whitelist of valid language codes accepted by this tool.
pub const VALID_LANG_CODES: &[&str] = &[
    "en", "id", "ms", "jv", "su", "ar", "zh", "ja", "ko", "es", "pt",
    "fr", "de", "it", "ru", "hi", "th", "vi", "tl", "tr", "pl", "nl",
    "sv", "da", "no", "fi",
];

/// Check if a language code is in the valid whitelist.
pub fn is_valid_lang(code: &str) -> bool {
    VALID_LANG_CODES.contains(&code)
}

/// Common language codes mapped to human-readable names.
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

/// Get human-readable language name from a 2-letter code.
pub fn get_language_name(code: &str) -> &str {
    LANGUAGE_NAMES
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, name)| *name)
        .unwrap_or("Unknown")
}

/// Suggest the best subtitle language based on video language and available subtitles.
pub fn suggest_subtitle_language(
    video_language: Option<&str>,
    available_manual: &[String],
    available_auto: &[String],
    llm_detected: Option<&str>,
) -> String {
    if let Some(lang) = llm_detected {
        if !lang.is_empty() {
            if available_manual.iter().any(|l| l == lang)
                || available_auto.iter().any(|l| l == lang)
            {
                return lang.to_string();
            }
        }
    }

    let vid_lang = video_language.unwrap_or("en");

    if available_manual.iter().any(|l| l == vid_lang) {
        return vid_lang.to_string();
    }
    if available_auto.iter().any(|l| l == vid_lang) {
        return vid_lang.to_string();
    }
    if available_manual.iter().any(|l| l == "en") {
        return "en".to_string();
    }
    if available_auto.iter().any(|l| l == "en") {
        return "en".to_string();
    }
    vid_lang.to_string()
}

/// Patterns that indicate a placeholder/unset API key.
const PLACEHOLDER_PATTERNS: &[&str] = &["your_", "your-", "changeme", "sk-your"];
const VALID_NON_PLACEHOLDERS: &[&str] = &["true", "false", "yes", "no"];

/// Detect placeholder API key values that haven't been replaced with real keys.
pub fn is_placeholder(value: &str) -> bool {
    let stripped = value.trim().to_lowercase();
    if stripped.is_empty() { return true; }
    if VALID_NON_PLACEHOLDERS.contains(&stripped.as_str()) { return false; }
    if PLACEHOLDER_PATTERNS.iter().any(|p| stripped.starts_with(&p.to_lowercase())) { return true; }
    if stripped.len() < 12 && !stripped.contains(' ') { return true; }
    false
}

#[derive(Debug, Clone)]
pub struct WatchConfig {
    pub groq_api_key: Option<String>,
    pub openai_api_key: Option<String>,
    pub config_dir: PathBuf,
}

impl WatchConfig {
    pub fn from_env() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        let config_dir = home.join(".config").join("watch");
        let _ = dotenvy::from_path(config_dir.join(".env"));
        Self {
            groq_api_key: std::env::var("GROQ_API_KEY")
                .ok()
                .filter(|s| !s.is_empty() && !is_placeholder(s)),
            openai_api_key: std::env::var("OPENAI_API_KEY")
                .ok()
                .filter(|s| !s.is_empty() && !is_placeholder(s)),
            config_dir,
        }
    }

    pub fn has_whisper_key(&self) -> bool {
        self.groq_api_key.is_some() || self.openai_api_key.is_some()
    }

    pub fn best_whisper_backend(&self) -> Option<&str> {
        if self.groq_api_key.is_some() {
            Some("groq")
        } else if self.openai_api_key.is_some() {
            Some("openai")
        } else {
            None
        }
    }
}
