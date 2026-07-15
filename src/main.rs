use clap::Parser;
use watch2::cli;
use watch2::config::{DetailMode, WatchConfig};
use watch2::download;
use watch2::frames;
use watch2::output::{FrameInfo, WatchReport};
use watch2::setup;
use watch2::timestamp::parse_time;
use watch2::transcript;
use watch2::whisper;
use std::path::PathBuf;
use std::collections::HashMap;

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
        }
        if !setup_status.has_api_key {
            eprintln!("⚠️  No Whisper API key (GROQ_API_KEY or OPENAI_API_KEY)");
            eprintln!("   Whisper fallback will be unavailable");
        }
        std::process::exit(3);
    }
    if setup_status.first_run {
        eprintln!("ℹ️  First run detected");
    }

    // Resolve detail mode
    let detail = cli.detail.as_ref().map(|d| match d {
        cli::DetailMode::Transcript => DetailMode::Transcript,
        cli::DetailMode::TranscriptMoments => DetailMode::TranscriptMoments,
        cli::DetailMode::Efficient => DetailMode::Efficient,
        cli::DetailMode::Balanced => DetailMode::Balanced,
        cli::DetailMode::TokenBurner => DetailMode::TokenBurner,
        cli::DetailMode::ScreenshotFirst => DetailMode::ScreenshotFirst,
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
        dl_result = download::fetch_captions(&cli.source, &download_dir, cli.cookies)?;
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

    // Phase 1: TranscriptMoments — first run (generate prompt and exit)
    if detail == DetailMode::TranscriptMoments {
        let moments_path = work.join("key_moments.json");
        if !moments_path.exists() {
            if transcript_segments.is_empty() {
                eprintln!("[watch2] ⚠️  TranscriptMoments requires a transcript. No subtitles found — falling through.");
            } else {
                eprintln!("[watch2] Phase 1: Generating moment detection prompt...");
                let transcript_text = watch2::moments::format_transcript_for_analysis(&transcript_segments);
                let prompt = watch2::moments::generate_prompt(
                    &transcript_text,
                    &dl_result.info.title,
                    dl_result.info.uploader.as_deref().unwrap_or("Unknown"),
                    dl_result.info.duration.unwrap_or(0.0),
                    cli.max_moments,
                    cli.min_moments,
                );
                let prompt_path = work.join("moments_prompt.txt");
                std::fs::write(&prompt_path, &prompt)?;
                eprintln!("[watch2] ✅ Moment prompt written to {}", prompt_path.display());
                eprintln!();
                eprintln!("📋 Agent workflow:");
                eprintln!("  1. Send the prompt to an LLM to identify key moments");
                eprintln!("  2. Save the LLM JSON response as an array to:");
                eprintln!("     {}", moments_path.display());
                eprintln!("  3. Re-run this command to extract frames at those moments");
                eprintln!();

                // Build minimal report with transcript only, then exit
                let report = WatchReport {
                    title: if dl_result.title.is_empty() || dl_result.title == "Unknown" {
                        cli.source.clone()
                    } else {
                        dl_result.title.clone()
                    },
                    source: cli.source.clone(),
                    detail: detail.to_string(),
                    uploader: dl_result.info.uploader.clone(),
                    language: dl_result.info.language.clone(),
                    engine: Some("transcript-moments-phase1".into()),
                    frames: vec![],
                    frames_dropped: 0,
                    transcript: transcript_segments,
                    transcript_source,
                    duration: dl_result.info.duration.unwrap_or(0.0),
                    working_dir: work.to_string_lossy().to_string(),
                    warnings: vec!["Phase 1 complete — waiting for key_moments.json".into()],
                    key_moments: None,
                    key_moment_stats: None,
                };

                match &cli.output {
                    cli::OutputFormat::Markdown => println!("{}", report.to_markdown()),
                    cli::OutputFormat::Json => println!("{}", report.to_json()),
                    cli::OutputFormat::Both => {
                        println!("{}", report.to_markdown());
                        let json_path = work.join("report.json");
                        let _ = std::fs::write(&json_path, report.to_json());
                    }
                }
                return Ok(());
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
            dl_result = download::download_video(&cli.source, &download_dir, cli.cookies)?;
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
                DetailMode::ScreenshotFirst => {
                    if transcript_segments.is_empty() {
                        eprintln!("[watch2] warning: no transcript for screenshot-first, falling back to uniform");
                        frames::extract_scene_or_uniform(
                            vp, &frames_dir, fps, max_frames, cli.resolution, max_frames,
                            focus_start, focus_end, !cli.no_dedup,
                        )?
                    } else {
                        let timestamps: Vec<f64> = transcript_segments.iter().map(|s| s.start).collect();
                        eprintln!("[watch2] engine: screenshot-first ({} transcript segments)", timestamps.len());
                        frames::extract_at_timestamps(
                            vp, &frames_dir, &timestamps, cli.resolution,
                            Some(max_frames), focus_start, focus_end,
                        )?
                    }
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
                DetailMode::TranscriptMoments => {
                    // Handled separately below — skip standard frame extraction
                    (vec![], frames::FrameMeta {
                        engine: "transcript-moments".into(),
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
    // Step 6b: Auto-moments — generate prompt for agent
    if cli.auto_moments && !transcript_segments.is_empty() {
        let transcript_text = watch2::moments::format_transcript_for_analysis(&transcript_segments);
        let prompt = watch2::moments::generate_prompt(
            &transcript_text,
            &dl_result.info.title,
            dl_result.info.uploader.as_deref().unwrap_or("Unknown"),
            dl_result.info.duration.unwrap_or(0.0),
            cli.max_moments,
            cli.min_moments,
        );
        let prompt_path = work.join("moments_prompt.txt");
        std::fs::write(&prompt_path, &prompt)?;
        eprintln!("[watch2] Moments prompt written to {}", prompt_path.display());
    }
    // Phase 2: TranscriptMoments — process key_moments.json and extract frames
    let moments_path = work.join("key_moments.json");
    let mut key_moments_raw: Vec<serde_json::Value> = Vec::new();
    let mut key_moment_stats: Option<watch2::output::KeyMomentStats> = None;

    if detail == DetailMode::TranscriptMoments && moments_path.exists() {
        eprintln!("[watch2] Phase 2: Processing key moments from {}", moments_path.display());
        match std::fs::read_to_string(&moments_path) {
            Ok(data) => {
                match serde_json::from_str::<Vec<watch2::moments::KeyMoment>>(&data) {
                    Ok(moments) => {
                        eprintln!("[watch2] Loaded {} key moments", moments.len());

                        // Extract timestamps from moments
                        let timestamps = watch2::moment_frames::get_timestamps_from_moments(&moments, None);
                        eprintln!("[watch2] {} unique timestamps for frame extraction", timestamps.len());

                        // Download video if needed for frame extraction
                        if video_path.is_none() && is_url {
                            eprintln!("[watch2] Downloading video for frame extraction...");
                            dl_result = download::download_video(&cli.source, &download_dir, cli.cookies)?;
                            video_path = dl_result.video_path;
                            if let Some(ref vp) = video_path {
                                if let Ok(meta) = frames::get_metadata(vp) {
                                    duration = meta.duration;
                                }
                            }
                        }

                        // Extract frames at moment timestamps (uncapped)
                        if let Some(ref vp) = video_path {
                            eprintln!("[watch2] Extracting frames at {} moment timestamps...", timestamps.len());
                            let focus_s = cli.start.as_deref().and_then(|s| parse_time(Some(s)));
                            let focus_e = cli.end.as_deref().and_then(|s| parse_time(Some(s)));
                            match frames::extract_at_timestamps(
                                vp, &frames_dir, &timestamps, cli.resolution,
                                None, // uncapped
                                focus_s, focus_e,
                            ) {
                                Ok((extracted, meta)) => {
                                    eprintln!(
                                        "[watch2] Extracted {} frames ({} dropped out of window)",
                                        extracted.len(), meta.dropped_out_of_window
                                    );

                                    // Link moments to frames
                                    let mut moments = moments;
                                    watch2::moment_frames::update_moments_with_frames(&mut moments, &extracted);

                                    // Update frames for report
                                    frames = extracted;

                                    // Serialize moments for report
                                    key_moments_raw = moments.iter().map(|m| {
                                        serde_json::to_value(m).unwrap_or_default()
                                    }).collect();

                                    // Calculate stats
                                    let mut by_reason: HashMap<String, usize> = HashMap::new();
                                    let mut by_priority: HashMap<u32, usize> = HashMap::new();
                                    for m in &key_moments_raw {
                                        if let Some(reason) = m.get("reason").and_then(|v| v.as_str()) {
                                            *by_reason.entry(reason.to_string()).or_insert(0) += 1;
                                        }
                                        if let Some(priority) = m.get("priority").and_then(|v| v.as_u64()) {
                                            *by_priority.entry(priority as u32).or_insert(0) += 1;
                                        }
                                    }
                                    key_moment_stats = Some(watch2::output::KeyMomentStats {
                                        total: key_moments_raw.len(),
                                        by_reason,
                                        by_priority,
                                    });
                                }
                                Err(e) => eprintln!("[watch2] ⚠️  Frame extraction failed: {}", e),
                            }
                        } else {
                            eprintln!("[watch2] ⚠️  No video available for frame extraction");
                        }
                    }
                    Err(e) => eprintln!("[watch2] ⚠️  Failed to parse key_moments.json: {}", e),
                }
            }
            Err(e) => eprintln!("[watch2] ⚠️  Failed to read key_moments.json: {}", e),
        }
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
        uploader: dl_result.info.uploader.clone(),
        language: dl_result.info.language.clone(),
        engine: Some(frame_meta.engine.clone()),
        frames,
        frames_dropped,
        transcript: transcript_segments,
        transcript_source,
        duration,
        working_dir: work.to_string_lossy().to_string(),
        warnings,
        key_moments: if key_moments_raw.is_empty() { None } else { Some(key_moments_raw) },
        key_moment_stats,
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

    // Step 8: Show stats if requested
    if cli.stats {
        let processing_time = start_time.elapsed().as_secs_f64();
        let stats = watch2::stats::collect_stats(&work, processing_time);
        let stats_output = match cli.stats_format {
            cli::StatsFormat::Compact => watch2::stats::format_stats_compact(&stats),
            cli::StatsFormat::Telegram => watch2::stats::format_stats_telegram(&stats),
        };
        eprintln!("\n{}", stats_output);
    }

    Ok(())
}
