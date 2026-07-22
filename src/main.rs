use clap::Parser;
use watch2::cache::VideoCache;
use watch2::cli;
use watch2::config::WatchConfig;
use watch2::pipeline::{self, PipelineContext};
use watch2::setup;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let start_time = std::time::Instant::now();
    let cli = cli::Cli::parse();
    let config = WatchConfig::from_env();

    // Early preflight check
    let setup_status = setup::check();
    if !setup_status.can_proceed {
        if !setup_status.missing_binaries.is_empty() {
            eprintln!("❌ Missing required binaries: {}", setup_status.missing_binaries.join(", "));
            eprintln!("   Install: apt install ffmpeg  (Linux)");
            std::process::exit(3);
        }
    }
    // API key check — warning only, not a blocker
    if !setup_status.has_api_key && !cli.no_whisper {
        eprintln!("⚠️  No Whisper API key (GROQ_API_KEY or OPENAI_API_KEY)");
        eprintln!("   Whisper fallback will be unavailable");
        eprintln!("   Use --no-whisper to suppress this warning");
    }
    if setup_status.first_run {
        eprintln!("ℹ️  First run detected");
    }

    // Save output format before moving cli into context
    let output_format = cli.output.clone();

    // Initialize cache
    let cache = if cli.no_cache {
        None
    } else {
        let cache_dir = cli.cache_dir.as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| dirs::cache_dir()
                .unwrap_or_default()
                .join("watch2"));
        match VideoCache::with_dir(cache_dir) {
            Ok(c) => {
                c.print_stats();
                Some(c)
            }
            Err(e) => {
                eprintln!("[watch2] cache init error: {} — proceeding without cache", e);
                None
            }
        }
    };

    // Create working directory
    let (work, _temp_dir) = match &cli.out_dir {
        Some(d) => (PathBuf::from(d), None),
        None => {
            let td = tempfile::tempdir()?;
            let path = td.path().to_path_buf();
            (path, Some(td))
        }
    };
    std::fs::create_dir_all(&work)?;
    eprintln!("[watch2] working dir: {}", work.display());

    let download_dir = work.join("download");
    let frames_dir = work.join("frames");

    // Build context and run pipeline
    let ctx = PipelineContext {
        cli,
        config,
        max_frames: 100,
        work: work.clone(),
        download_dir,
        frames_dir,
        start_time,
        cache,
    };

    let report = pipeline::run(ctx).await?;

    // Output report
    match &output_format {
        cli::OutputFormat::Markdown => println!("{}", report.to_markdown()),
        cli::OutputFormat::Json => println!("{}", report.to_json()),
        cli::OutputFormat::Both => {
            println!("{}", report.to_markdown());
            let json_path = work.join("report.json");
            match std::fs::write(&json_path, report.to_json()) {
                Ok(()) => eprintln!("[watch2] report JSON: {}", json_path.display()),
                Err(e) => eprintln!("[watch2] failed to write JSON: {}", e),
            }
        }
    }

    Ok(())
}
