use clap::{Parser, ValueEnum};

/// Watch a video and analyze it
#[derive(Parser)]
#[command(name = "watch", about = "Download, extract frames, and transcribe a video")]
pub struct Cli {
    /// Video URL or local file path
    pub source: String,

    /// Detail mode: transcript, efficient, balanced, token-burner
    #[arg(long)]
    pub detail: Option<DetailMode>,

    /// Frame cap override
    #[arg(long)]
    pub max_frames: Option<u32>,

    /// Frame width in pixels (default 512)
    #[arg(long, default_value_t = 512)]
    pub resolution: u32,

    /// Override auto-fps (max 2.0)
    #[arg(long)]
    pub fps: Option<f32>,

    /// Comma-separated timestamps to grab frames at
    #[arg(long)]
    pub timestamps: Option<String>,

    /// Range start (SS, MM:SS, or HH:MM:SS)
    #[arg(long)]
    pub start: Option<String>,

    /// Range end (SS, MM:SS, or HH:MM:SS)
    #[arg(long)]
    pub end: Option<String>,

    /// Working directory
    #[arg(long)]
    pub out_dir: Option<String>,

    /// Force Whisper backend
    #[arg(long)]
    pub whisper: Option<WhisperBackend>,

    /// Disable Whisper fallback
    #[arg(long)]
    pub no_whisper: bool,

    /// Disable near-duplicate frame removal
    #[arg(long)]
    pub no_dedup: bool,

    /// Output format: markdown, json, or both
    #[arg(long, value_enum, default_value_t = OutputFormat::Markdown)]
    pub output: OutputFormat,

    /// Keep downloaded video after processing
    #[arg(long)]
    pub keep_video: bool,

    /// Use Chrome cookies for authenticated YouTube sessions (opt-in, breaks android_vr)
    #[arg(long)]
    pub cookies: bool,

    /// Auto-generate moment detection prompt
    #[arg(long)]
    pub auto_moments: bool,

    /// Maximum moments to detect
    #[arg(long, default_value_t = 50)]
    pub max_moments: u32,

    /// Minimum moments to detect
    #[arg(long)]
    pub min_moments: Option<u32>,

    /// Show processing stats at the end
    #[arg(long)]
    pub stats: bool,

    /// Stats display format: telegram (rich) or compact (single line)
    #[arg(long, value_enum, default_value_t = StatsFormat::Telegram)]
    pub stats_format: StatsFormat,

    /// Disable download cache
    #[arg(long)]
    pub no_cache: bool,

    /// Custom cache directory
    #[arg(long)]
    pub cache_dir: Option<String>,
}

#[derive(Clone, Debug, PartialEq, ValueEnum)]
pub enum DetailMode {
    Transcript,
    TranscriptMoments,
    Efficient,
    Balanced,
    TokenBurner,
    ScreenshotFirst,
}

impl std::fmt::Display for DetailMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DetailMode::Transcript => write!(f, "transcript"),
            DetailMode::TranscriptMoments => write!(f, "transcript-moments"),
            DetailMode::Efficient => write!(f, "efficient"),
            DetailMode::Balanced => write!(f, "balanced"),
            DetailMode::TokenBurner => write!(f, "token-burner"),
            DetailMode::ScreenshotFirst => write!(f, "screenshot-first"),
        }
    }
}

#[derive(Clone, Debug, ValueEnum)]
pub enum WhisperBackend {
    Groq,
    Openai,
}

#[derive(Clone, Debug, PartialEq, ValueEnum)]
pub enum OutputFormat {
    Markdown,
    Json,
    Both,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Markdown => write!(f, "markdown"),
            Self::Json => write!(f, "json"),
            Self::Both => write!(f, "both"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, ValueEnum)]
pub enum StatsFormat {
    Telegram,
    Compact,
}

impl std::fmt::Display for StatsFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Telegram => write!(f, "telegram"),
            Self::Compact => write!(f, "compact"),
        }
    }
}
