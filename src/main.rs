use clap::Parser;
use watch2::cli;
use watch2::config::{DetailMode, WatchConfig};
use watch2::download;
use watch2::frames;
use watch2::output::{FrameInfo, WatchReport};
use watch2::timestamp::parse_time;
use watch2::transcript;
use watch2::whisper;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = cli::Cli::parse();
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
        None => tempfile::tempdir()?.keep(),
    };
    std::fs::create_dir_all(&work)?;
    eprintln!("[watch2] working dir: {}", work.display());

    let download_dir = work.join("download");
    let frames_dir = work.join("frames");

    // Step 1: Resolve source
    let is_url = download::is_url(&cli.source);
    let mut dl_result: download::DownloadResult;

    if is_url {
        eprintln!("[watch2] fetching metadata/captions...");
        dl_result = download::fetch_captions(&cli.source, &download_dir)?;
    } else {
        dl_result = download::resolve_local(&cli.source)?;
    }

    // Step 2: Parse transcript from captions
    let mut transcript_segments: Vec<watch2::output::TranscriptSegment> = Vec::new();
    let mut transcript_source = String::from("none");
    if let Some(ref sub_path) = dl_result.subtitle_path {
        eprintln!("[watch2] parsing subtitles from {}", sub_path.display());
        match transcript::parse_subtitle_file(sub_path) {
            Ok(segs) => {
                transcript_segments = segs;
                transcript_source = "captions".to_string();
            }
            Err(e) => {
                eprintln!("[watch2] subtitle parse error: {}", e);
            }
        }
    }
    // Filter transcript to focus range if specified
    let focus_start = cli.start.as_deref().and_then(|s| parse_time(Some(s)));
    let focus_end = cli.end.as_deref().and_then(|s| parse_time(Some(s)));
    if focus_start.is_some() || focus_end.is_some() {
        transcript_segments = transcript::filter_by_range(&transcript_segments, focus_start, focus_end);
    }

    // Step 3: Get video metadata
    let mut duration = 0.0;
    let mut video_path: Option<PathBuf> = dl_result.video_path.take();

    if let Some(ref vp) = video_path {
        match frames::get_metadata(vp) {
            Ok(meta) => {
                duration = meta.duration;
                eprintln!("[watch2] duration: {:.1}s", duration);
            }
            Err(e) => {
                eprintln!("[watch2] metadata error: {}", e);
            }
        }
    }

    // Step 4: Download video if needed (for frame extraction)
    let mut frames: Vec<FrameInfo> = Vec::new();
    let mut frames_dropped = 0u32;
    let mut focused = false;
    let mut frame_meta = frames::FrameMeta {
        engine: "none".into(),
        candidate_count: 0,
        selected_count: 0,
        deduped_count: 0,
        fallback: false,
        dropped_out_of_window: 0,
    };

    if detail != DetailMode::Transcript {
        // Need to download if we don't have video yet
        if video_path.is_none() && is_url {
            eprintln!("[watch2] downloading video...");
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
            // Determine focus mode
            let focus_start = cli.start.as_ref().and_then(|s| parse_time(Some(s)));
            let focus_end = cli.end.as_ref().and_then(|s| parse_time(Some(s)));
            focused = focus_start.is_some() && focus_end.is_some();

            // Parse cue timestamps if provided
            let cue_timestamps: Vec<f64> = cli.timestamps
                .as_ref()
                .map(|t| t.split(',').filter_map(|s| parse_time(Some(s.trim()))).collect())
                .unwrap_or_default();

            let mut cue_frames: Vec<FrameInfo> = Vec::new();
            let mut cue_meta: frames::FrameMeta = frames::FrameMeta {
                engine: "none".into(),
                candidate_count: 0,
                selected_count: 0,
                deduped_count: 0,
                fallback: false,
                dropped_out_of_window: 0,
            };

            if !cue_timestamps.is_empty() {
                match frames::extract_at_timestamps(
                    vp, &frames_dir, &cue_timestamps, cli.resolution,
                    Some(max_frames), focus_start, focus_end,
                ) {
                    Ok((extracted, meta)) => {
                        if meta.dropped_out_of_window > 0 {
                            eprintln!(
                                "[watch2] {} cue timestamp(s) outside focus range — dropped",
                                meta.dropped_out_of_window
                            );
                        }
                        cue_frames = extracted;
                        cue_meta = meta;
                    }
                    Err(e) => eprintln!("[watch2] cue frame extraction error: {}", e),
                }
            }

            // Calculate fps
            let fps = if let Some(f) = cli.fps {
                f.min(2.0)
            } else if let (Some(_), Some(_)) = (focus_start, focus_end) {
                let focus_dur = focus_end.unwrap() - focus_start.unwrap_or(0.0);
                frames::auto_fps_focus(focus_dur, max_frames)
            } else {
                frames::auto_fps(duration, max_frames)
            };

            // Dispatch to the correct frame extraction engine based on detail mode
            let (mut extracted, meta) = match detail {
                DetailMode::Efficient => {
                    eprintln!("[watch2] engine: keyframe (efficient mode)");
                    frames::extract_keyframes(
                        vp, &frames_dir, cli.resolution, max_frames,
                        focus_start, focus_end, !cli.no_dedup,
                    )?
                }
                DetailMode::Balanced => {
                    eprintln!("[watch2] engine: scene-or-uniform ({:.2} fps, cap: {})", fps, max_frames);
                    frames::extract_scene_or_uniform(
                        vp, &frames_dir, fps, max_frames, cli.resolution, max_frames,
                        focus_start, focus_end, !cli.no_dedup,
                    )?
                }
                DetailMode::TokenBurner => {
                    eprintln!("[watch2] engine: two-pass ({:.2} fps, cap: {})", fps, max_frames);
                    frames::extract_two_pass(
                        vp, &frames_dir, fps, max_frames, cli.resolution,
                        focus_start, focus_end, !cli.no_dedup,
                    )?
                }
                DetailMode::Transcript => {
                    // No frames — transcript-only mode
                    (vec![], frames::FrameMeta {
                        engine: "none".into(),
                        candidate_count: 0,
                        selected_count: 0,
                        deduped_count: 0,
                        fallback: false,
                        dropped_out_of_window: 0,
                    })
                }
            };

            // Merge cue frames (pinned against cap)
            if !cue_frames.is_empty() {
                extracted.extend(cue_frames);
                extracted.sort_by(|a, b| a.timestamp.partial_cmp(&b.timestamp).unwrap());
            }

            frames = extracted;
            frames_dropped = meta.deduped_count;
            eprintln!(
                "[watch2] engine: {}, candidates: {}, selected: {}, dropped: {}",
                meta.engine, meta.candidate_count, meta.selected_count, meta.deduped_count
            );
            if cue_meta.selected_count > 0 {
                eprintln!(
                    "[watch2] cue timestamps: {} extracted ({} dropped out of window)",
                    cue_meta.selected_count, cue_meta.dropped_out_of_window
                );
            }
            frame_meta = meta;
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
                eprintln!("[watch2] transcribing via {}...", backend);
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
                                eprintln!("[watch2] transcript: {} segments", transcript_segments.len());
                            }
                            Err(e) => {
                                eprintln!("[watch2] whisper error: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("[watch2] audio extraction error: {}", e);
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

    // Step 7: Cleanup downloaded video
    if !cli.keep_video {
        if let Some(ref vp) = video_path {
            if dl_result.downloaded {
                let size_mb = std::fs::metadata(vp)
                    .map(|m| m.len() / (1024 * 1024))
                    .unwrap_or(0);
                std::fs::remove_file(vp).ok();
                eprintln!("[watch2] cleaned up video ({} MB)", size_mb);
            }
        }
    }

    // Cleanup audio temp files
    let audio_tmp = work.join("audio.mp3");
    if audio_tmp.exists() {
        std::fs::remove_file(&audio_tmp).ok();
    }

    // Step 7: Generate report
    let mut warnings = Vec::new();

    // Token-burner with too many frames
    if detail == DetailMode::TokenBurner && frames.len() > 250 {
        warnings.push(format!(
            "{} frames selected. This may use a large number of image tokens.",
            frames.len()
        ));
    }

    // Long video with sparse coverage
    if !focused && duration > 600.0 && detail != DetailMode::Transcript && detail != DetailMode::TokenBurner {
        warnings.push(format!(
            "This is a {:.0}-minute video. Frame coverage is sparse under `{}` detail.",
            duration / 60.0, detail
        ));
    }

    // No transcript
    if transcript_segments.is_empty() {
        warnings.push("No transcript available — proceed with frames only.".into());
    }

    // Fallback engine used
    if frame_meta.fallback {
        warnings.push(format!(
            "Used {} fallback (detected {} candidates, below minimum).",
            frame_meta.engine, frame_meta.candidate_count
        ));
    }

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
        warnings,
    };

    match &cli.output {
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
