use clap::{Parser, ValueEnum};

/// Validate resolution: 128-4096 pixels
fn validate_resolution(s: &str) -> Result<u32, String> {
    let val: u32 = s.parse().map_err(|_| format!("'{s}' is not a valid number"))?;
    if val < 128 {
        return Err("resolution must be at least 128 pixels".to_string());
    }
    if val > 4096 {
        return Err("resolution capped at 4096 pixels".to_string());
    }
    Ok(val)
}

/// Watch a video and analyze it
#[derive(Parser)]
#[command(name = "watch", about = "Download, extract frames, and transcribe a video")]
pub struct Cli {
    /// Video URL or local file path
    pub source: String,

    /// Frame width in pixels (128-4096, default 512)
    #[arg(long, default_value_t = 512, value_parser = validate_resolution)]
    pub resolution: u32,

    /// Working directory
    #[arg(long)]
    pub out_dir: Option<String>,

    /// Keep downloaded video after processing
    #[arg(long)]
    pub keep_video: bool,

    /// Use Chrome cookies for authenticated YouTube sessions (opt-in, breaks android_vr)
    #[arg(long)]
    pub cookies: bool,

    /// Disable Whisper fallback
    #[arg(long)]
    pub no_whisper: bool,

    /// Disable near-duplicate frame removal
    #[arg(long)]
    pub no_dedup: bool,

    /// Output format: markdown, json, or both
    #[arg(long, value_enum, default_value_t = OutputFormat::Markdown)]
    pub output: OutputFormat,

    /// Disable download cache
    #[arg(long)]
    pub no_cache: bool,

    /// Custom cache directory
    #[arg(long)]
    pub cache_dir: Option<String>,

    /// Comma-separated timestamps for cue frame extraction (e.g. "00:30,01:15,02:45")
    /// When set, extracts frames ONLY at these timestamps (skips uniform extraction)
    #[arg(long)]
    pub timestamps: Option<String>,
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
