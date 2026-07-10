mod cli;
mod config;
mod dedup;
mod download;
mod error;
mod frames;
mod output;
mod scene;
mod setup;
mod timestamp;
mod transcript;
mod whisper;

use clap::Parser;
use cli::Cli;
use config::{DetailMode, WatchConfig};
use output::{FrameInfo, WatchReport};
use std::path::{Path, PathBuf};
use timestamp::parse_time;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    let config = WatchConfig::from_env();

    // Resolve detail mode
    let detail = cli.detail.as_ref().map(|d| match d {
        cli::DetailMode::Transcript => DetailMode::Transcript,
        cli::DetailMode::Efficient => DetailMode::Efficient,
        cli::DetailMode::Balanced => DetailMode::Balanced,
        cli::DetailMode::TokenBurner => DetailMode::TokenBurner,
    }).unwrap_or_else(|| config.detail.clone());

    // Frame cap
    let max_frames = cli.max_frames.unwrap_or_else(|| {
        config.frame_cap(&detail).unwrap_or(100)
    });

    // Create working directory
    let work = match &cli.out_dir {
        Some(d) => PathBuf::from(d),
        None => tempfile::tempdir()?.into_path(),
    };
    std::fs::create_dir_all(&work)?;
    eprintln!("[watch-rs] working dir: {}", work.display());

    let download_dir = work.join("download");
    let frames_dir = work.join("frames");

    // Step 1: Resolve source
    let is_url = download::is_url(&cli.source);
    let mut dl_result: download::DownloadResult;

    if is_url {
        eprintln!("[watch-rs] fetching metadata/captions...");
        dl_result = download::fetch_captions(&cli.source, &download_dir)?;
    } else {
        dl_result = download::resolve_local(&cli.source)?;
    }

    // Step 2: Parse transcript from captions
    let mut transcript_segments: Vec<output::TranscriptSegment> = Vec::new();
    let mut transcript_source = String::from("none");

    if let Some(ref sub_path) = dl_result.subtitle_path {
        eprintln!("[watch-rs] parsing subtitles from {}", sub_path.display());
        match transcript::parse_subtitle_file(sub_path) {
            Ok(segs) => {
                transcript_segments = segs;
                transcript_source = "captions".to_string();
            }
            Err(e) => {
                eprintln!("[watch-rs] subtitle parse error: {}", e);
            }
        }
    }

    // Step 3: Get video metadata
    let mut duration = 0.0;
    let mut video_path: Option<PathBuf> = dl_result.video_path.take();

    if let Some(ref vp) = video_path {
        match frames::get_metadata(vp) {
            Ok(meta) => {
                duration = meta.duration;
                eprintln!("[watch-rs] duration: {:.1}s", duration);
            }
            Err(e) => {
                eprintln!("[watch-rs] metadata error: {}", e);
            }
        }
    }

    // Step 4: Download video if needed (for frame extraction)
    let mut frames: Vec<FrameInfo> = Vec::new();
    let mut frames_dropped = 0u32;

    if detail != DetailMode::Transcript {
        // Need to download if we don't have video yet
        if video_path.is_none() && is_url {
            eprintln!("[watch-rs] downloading video...");
            dl_result = download::download_video(&cli.source, &download_dir)?;
            video_path = dl_result.video_path;

            // Get metadata after download
            if let Some(ref vp) = video_path {
                if let Ok(meta) = frames::get_metadata(vp) {
                    duration = meta.duration;
                }
            }
        }

        if let Some(ref vp) = video_path {
            // Parse cue timestamps if provided
            let cue_timestamps: Vec<f64> = cli.timestamps
                .as_ref()
                .map(|t| t.split(',').filter_map(|s| parse_time(Some(s.trim()))).collect())
                .unwrap_or_default();

            // Determine focus mode
            let focus_start = cli.start.as_ref().and_then(|s| parse_time(Some(s)));
            let focus_end = cli.end.as_ref().and_then(|s| parse_time(Some(s)));

            // Calculate fps
            let fps = if let Some(f) = cli.fps {
                f.min(2.0)
            } else if let (Some(_), Some(_)) = (focus_start, focus_end) {
                // Focus mode: denser fps
                let focus_dur = focus_end.unwrap() - focus_start.unwrap_or(0.0);
                frames::auto_fps_focus(focus_dur, max_frames)
            } else {
                frames::auto_fps(duration, max_frames)
            };

            eprintln!("[watch-rs] extracting frames at {:.2} fps (cap: {})", fps, max_frames);

            // Extract frames
            match frames::extract_frames(vp, &frames_dir, fps, cli.resolution, max_frames) {
                Ok(mut extracted) => {
                    // Filter by focus range if specified
                    if let (Some(start), Some(end)) = (focus_start, focus_end) {
                        extracted.retain(|f| f.timestamp >= start && f.timestamp <= end);
                    }

                    // Dedup unless disabled
                    if !cli.no_dedup {
                        frames_dropped = dedup::dedup_frames(&mut extracted);
                        eprintln!("[watch-rs] dedup: dropped {} frames", frames_dropped);
                    }

                    frames = extracted;
                    eprintln!("[watch-rs] {} frames extracted", frames.len());
                }
                Err(e) => {
                    eprintln!("[watch-rs] frame extraction error: {}", e);
                }
            }
        }
    }

    // Step 5: Whisper fallback if no transcript
    if transcript_segments.is_empty() && !cli.no_whisper {
        let backend = cli.whisper.as_ref().map(|b| match b {
            cli::WhisperBackend::Groq => "groq",
            cli::WhisperBackend::Openai => "openai",
        }).unwrap_or_else(|| config.best_whisper_backend().unwrap_or("none"));

        if backend != "none" {
            let api_key = match backend {
                "groq" => config.groq_api_key.as_deref(),
                "openai" => config.openai_api_key.as_deref(),
                _ => None,
            };

            if let (Some(key), Some(vp)) = (api_key, video_path.as_ref()) {
                eprintln!("[watch-rs] transcribing via {}...", backend);
                match whisper::extract_audio(vp, &work) {
                    Ok(audio_path) => {
                        let result = match backend {
                            "groq" => whisper::transcribe_groq(&audio_path, key).await,
                            _ => whisper::transcribe_openai(&audio_path, key).await,
                        };
                        match result {
                            Ok(segs) => {
                                transcript_segments = segs;
                                transcript_source = format!("whisper ({})", backend);
                                eprintln!("[watch-rs] transcript: {} segments", transcript_segments.len());
                            }
                            Err(e) => {
                                eprintln!("[watch-rs] whisper error: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("[watch-rs] audio extraction error: {}", e);
                    }
                }
            }
        }
    }

    // Step 6: Filter transcript to focus range
    if let (Some(start), Some(end)) = (
        cli.start.as_ref().and_then(|s| parse_time(Some(s))),
        cli.end.as_ref().and_then(|s| parse_time(Some(s))),
    ) {
        transcript_segments.retain(|s| s.end >= start && s.start <= end);
    }

    // Step 7: Generate report
    let title = if dl_result.title.is_empty() || dl_result.title == "Unknown" {
        cli.source.clone()
    } else {
        dl_result.title.clone()
    };

    let report = WatchReport {
        title,
        source: cli.source.clone(),
        detail: detail.to_string(),
        frames,
        frames_dropped,
        transcript: transcript_segments,
        transcript_source,
        duration,
        working_dir: work.to_string_lossy().to_string(),
    };

    println!("{}", report.to_markdown());

    Ok(())
}
