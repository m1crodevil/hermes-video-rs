use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum DetailMode {
    Transcript,
    Efficient,
    Balanced,
    TokenBurner,
}
impl Default for DetailMode {
    fn default() -> Self {
        Self::Balanced
    }
}
impl std::fmt::Display for DetailMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transcript => write!(f, "transcript"),
            Self::Efficient => write!(f, "efficient"),
            Self::Balanced => write!(f, "balanced"),
            Self::TokenBurner => write!(f, "token-burner"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WatchConfig {
    pub detail: DetailMode,
    pub groq_api_key: Option<String>,
    pub openai_api_key: Option<String>,
    pub config_dir: PathBuf,
}

impl WatchConfig {
    pub fn from_env() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        let config_dir = home.join(".config").join("watch");
        let _ = dotenvy::from_path(config_dir.join(".env"));
        let _ = dotenvy::from_path(".env");
        let detail = match std::env::var("WATCH_DETAIL").unwrap_or_default().as_str() {
            "transcript" => DetailMode::Transcript,
            "efficient" => DetailMode::Efficient,
            "token-burner" => DetailMode::TokenBurner,
            _ => DetailMode::Balanced,
        };
        Self {
            detail,
            groq_api_key: std::env::var("GROQ_API_KEY").ok().filter(|s| !s.is_empty()),
            openai_api_key: std::env::var("OPENAI_API_KEY")
                .ok()
                .filter(|s| !s.is_empty()),
            config_dir,
        }
    }
    pub fn frame_cap(&self, detail: &DetailMode) -> Option<u32> {
        match detail {
            DetailMode::Transcript => None,
            DetailMode::Efficient => Some(50),
            DetailMode::Balanced => Some(100),
            DetailMode::TokenBurner => None,
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
