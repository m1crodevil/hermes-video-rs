use crate::cli;
use crate::cache::VideoCache;
use crate::config::{DetailMode, WatchConfig};
use crate::download;
use crate::frames;
use crate::output::{FrameInfo, WatchReport};
use crate::timestamp::parse_time;
use crate::transcript;
use crate::whisper;
use std::collections::HashMap;
use std::path::PathBuf;

/// All state needed to run the analysis pipeline.
pub struct PipelineContext {
    pub cli: cli::Cli,
    pub config: WatchConfig,
    pub detail: DetailMode,
    pub max_frames: u32,
    pub work: PathBuf,
    pub download_dir: PathBuf,
    pub frames_dir: PathBuf,
    pub start_time: std::time::Instant,
    pub cache: Option<VideoCache>,
}

/// Run the full analysis pipeline. Returns the report for output.
pub async fn run(ctx: PipelineContext) -> anyhow::Result<WatchReport> {
    let PipelineContext {
        cli,
        config,
        detail,
        max_frames,
        work,
        download_dir,
        frames_dir,
        start_time,
        mut cache,
    } = ctx;

    // ── Step 1: Resolve source ──────────────────────────────────────────
    let is_url = download::is_url(&cli.source);
    let mut dl_result: download::DownloadResult;

    if is_url {
        eprintln!("[watch2] fetching metadata/captions...");
        dl_result = download::fetch_captions(&cli.source, &download_dir, cli.cookies, None)?;
        // Store subtitles in cache
        if let Some(ref mut c) = cache {
            if let Some(ref sp) = dl_result.subtitle_path {
                let _ = c.store_subtitles(&cli.source, "en", sp);
            }
            let _ = c.store_info(&cli.source, &dl_result.info);
        }
    } else {
        dl_result = download::resolve_local(&cli.source)?;
    }

    // ── Step 1b: LLM language detection ────────────────────────────────
    let mut llm_lang: Option<String> = None;
    if is_url {
        llm_lang = crate::llm::detect_language_llm(
            &dl_result.info.title,
            dl_result.info.description.as_deref(),
            &config,
        )
        .await;
        if let Some(ref lang) = llm_lang {
            eprintln!("[watch2] LLM detected language: {}", lang);
        }
    }

    // ── Step 2: Parse transcript from captions ──────────────────────────
    let mut transcript_segments: Vec<crate::output::TranscriptSegment> = Vec::new();
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
        transcript_segments =
            transcript::filter_by_range(&transcript_segments, focus_start, focus_end);
    }

    // ── Step 2b: Scene detection (fused mode) ──────────────────────────────
    let mut fused_moments: Vec<crate::fusion::FusedMoment> = Vec::new();
    let mut scene_text = String::new();
    let mut fusion_text = String::new();
    let mut scene_count: Option<usize> = None;

    if cli.fuse_scenes && !transcript_segments.is_empty() {
        if let Some(ref vp) = dl_result.video_path {
            eprintln!("[watch2] Running fused scene+transcript analysis...");
            let meta = frames::get_metadata(vp);
            let dur = meta.as_ref().map(|m| m.duration).unwrap_or(0.0);
            // Use cli.fps if set, otherwise default to 30.0 for scene detection
            let fps = cli.fps.unwrap_or(30.0) as f64;

            match crate::scene_detect::detect(vp, fps, dur) {
                Ok(result) => {
                    eprintln!("[watch2] Detected {} scenes ({}ms)", result.total_scenes(), result.detection_time_ms);
                    scene_text = crate::fusion::format_scene_changes_for_prompt(&result.boundaries);
                    fused_moments = crate::fusion::fuse_scenes_and_transcript(
                        &result.boundaries,
                        &transcript_segments,
                        dur,
                    );
                    fusion_text = crate::fusion::format_fusion_data_for_prompt(&fused_moments);
                    scene_count = Some(result.total_scenes());
                    eprintln!("[watch2] {} fused moments generated", fused_moments.len());
                }
                Err(e) => {
                    eprintln!("[watch2] Scene detection failed: {}", e);
                }
            }
        }
    }

    // ── Phase 1: TranscriptMoments — first run (generate prompt & exit) ─
    if detail == DetailMode::TranscriptMoments {
        let moments_path = work.join("key_moments.json");
        if !moments_path.exists() {
            if transcript_segments.is_empty() {
                eprintln!("[watch2] ⚠️  TranscriptMoments requires a transcript. No subtitles found — falling through.");
                // Check if .json3 files exist despite no transcript (detection bug indicator)
                let has_stale_subs = download_dir.read_dir().map_or(false, |entries| {
                    entries.flatten().any(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        name.ends_with(".json3") || name.ends_with(".vtt")
                    })
                });
                if has_stale_subs {
                    eprintln!("[watch2] 💡 Subtitle files exist in {:?} but weren't detected.", download_dir);
                    eprintln!("[watch2]    This may indicate a subtitle detection bug. Check manually:");
                    eprintln!("[watch2]    ls {:?}", download_dir.join("video.*.json3"));
                }
            } else {
                return run_transcript_moments_phase1(
                    &cli,
                    &detail,
                    &work,
                    &transcript_segments,
                    &transcript_source,
                    &dl_result,
                    &scene_text,
                    &fusion_text,
                );
            }
        }
    }

    // ── Step 3: Get video metadata ──────────────────────────────────────
    let mut duration = 0.0;
    let mut video_path: Option<PathBuf> = dl_result.video_path.clone();

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

    // ── Step 4: Frame extraction ────────────────────────────────────────
    let mut frame_vec: Vec<FrameInfo> = Vec::new();
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
        // Download if needed
        if video_path.is_none() && is_url {
            eprintln!("[watch2] downloading video...");
            dl_result = download::download_video(&cli.source, &download_dir, cli.cookies, llm_lang.as_deref())?;
            video_path = dl_result.video_path.clone();

            if let Some(ref vp) = video_path {
                if let Ok(meta) = frames::get_metadata(vp) {
                    duration = meta.duration;
                }
                // Store video in cache
                if let Some(ref mut c) = cache {
                    let _ = c.store_video(&cli.source, vp);
                }
            }
        }

        if let Some(ref vp) = video_path {
            let result = extract_frames_inner(
                vp, &cli, &detail, &frames_dir, max_frames, duration,
                &transcript_segments,
            );
            match result {
                Ok(r) => {
                    frame_vec = r.frames;
                    frames_dropped = r.dropped;
                    focused = r.focused;
                    frame_meta = r.meta;
                }
                Err(e) => eprintln!("[watch2] frame extraction error: {}", e),
            }
        }
    }

    // ── Step 5: Whisper fallback ────────────────────────────────────────
    if transcript_segments.is_empty() && !cli.no_whisper {
        run_whisper_fallback(
            &cli,
            &config,
            &work,
            &video_path,
            &mut transcript_segments,
            &mut transcript_source,
        )
        .await;
    }

    // ── Step 5b: Explain when no transcript and no whisper ──────────────
    if transcript_segments.is_empty() {
        if cli.no_whisper {
            eprintln!("⚠️  No subtitles found for this video.");
            eprintln!("   --no-whisper was set, so Whisper fallback was skipped.");
            eprintln!("   No transcript will be available for this video.");
        } else if !config.has_whisper_key() {
            eprintln!("⚠️  No subtitles found for this video.");
            eprintln!("   Whisper API key required for transcription.");
            eprintln!("   Set GROQ_API_KEY or OPENAI_API_KEY in ~/.config/watch/.env");
            eprintln!("   Or use --no-whisper to skip (no transcript available)");
        }
    }

    // ── Step 6: Filter transcript to focus range ────────────────────────
    if let (Some(start), Some(end)) = (
        cli.start.as_ref().and_then(|s| parse_time(Some(s))),
        cli.end.as_ref().and_then(|s| parse_time(Some(s))),
    ) {
        transcript_segments.retain(|s| s.end >= start && s.start <= end);
    }

    // ── Step 6b: Auto-moments ──────────────────────────────────────────
    if cli.auto_moments && !transcript_segments.is_empty() {
        let transcript_text =
            crate::moments::format_transcript_for_analysis(&transcript_segments);
        let prompt = crate::moments::generate_prompt(
            &transcript_text,
            &dl_result.info.title,
            dl_result.info.uploader.as_deref().unwrap_or("Unknown"),
            dl_result.info.duration.unwrap_or(0.0),
            cli.max_moments,
            cli.min_moments,
        );
        let prompt_path = work.join("moments_prompt.txt");
        std::fs::write(&prompt_path, prompt)?;
        eprintln!("[watch2] Moments prompt written to {}", prompt_path.display());
    }

    // ── Phase 2: TranscriptMoments — process key_moments.json ──────────
    let moments_path = work.join("key_moments.json");
    let mut key_moments_raw: Vec<serde_json::Value> = Vec::new();
    let mut key_moment_stats: Option<crate::output::KeyMomentStats> = None;

    if detail == DetailMode::TranscriptMoments && moments_path.exists() {
        let result = run_transcript_moments_phase2(
            &cli,
            &detail,
            &work,
            &frames_dir,
            &download_dir,
            &mut dl_result,
            &mut video_path,
            &mut duration,
            &mut frame_vec,
            &mut key_moments_raw,
            &mut key_moment_stats,
            is_url,
            llm_lang.as_deref(),
        );
        // Phase 2 may have failed, but we continue with what we have
        if let Err(e) = result {
            eprintln!("[watch2] ⚠️  Phase 2 error: {}", e);
        }
    }

    // ── Step 7: Cleanup ─────────────────────────────────────────────────
    cleanup(&cli, &work, &video_path, &dl_result);

    // ── Step 8: Generate report ─────────────────────────────────────────
    let report = build_report(
        &cli,
        &detail,
        &work,
        &dl_result,
        frame_vec,
        frames_dropped,
        &frame_meta,
        transcript_segments,
        &transcript_source,
        duration,
        focused,
        key_moments_raw,
        key_moment_stats,
        fused_moments,
        scene_count,
    );

    // ── Step 9: Show stats ──────────────────────────────────────────────
    if cli.stats {
        let processing_time = start_time.elapsed().as_secs_f64();
        let stats = crate::stats::collect_stats(&work, processing_time);
        let stats_output = match cli.stats_format {
            cli::StatsFormat::Compact => crate::stats::format_stats_compact(&stats),
            cli::StatsFormat::Telegram => crate::stats::format_stats_telegram(&stats),
        };
        eprintln!("\n{}", stats_output);
    }

    Ok(report)
}

// ─── Internal helpers ───────────────────────────────────────────────────────

struct FrameExtractionResult {
    frames: Vec<FrameInfo>,
    dropped: u32,
    focused: bool,
    meta: frames::FrameMeta,
}

fn extract_frames_inner(
    vp: &std::path::Path,
    cli: &cli::Cli,
    detail: &DetailMode,
    frames_dir: &PathBuf,
    max_frames: u32,
    duration: f64,
    transcript_segments: &[crate::output::TranscriptSegment],
) -> anyhow::Result<FrameExtractionResult> {
    let focus_start = cli.start.as_ref().and_then(|s| parse_time(Some(s)));
    let focus_end = cli.end.as_ref().and_then(|s| parse_time(Some(s)));
    let focused = focus_start.is_some() && focus_end.is_some();

    // Parse cue timestamps
    let cue_timestamps: Vec<f64> = cli
        .timestamps
        .as_ref()
        .map(|t| {
            t.split(',')
                .filter_map(|s| parse_time(Some(s.trim())))
                .collect()
        })
        .unwrap_or_default();

    let mut cue_frames: Vec<FrameInfo> = Vec::new();
    let mut cue_meta = frames::FrameMeta {
        engine: "none".into(),
        candidate_count: 0,
        selected_count: 0,
        deduped_count: 0,
        fallback: false,
        dropped_out_of_window: 0,
    };

    if !cue_timestamps.is_empty() {
        match frames::extract_at_timestamps(
            vp,
            frames_dir,
            &cue_timestamps,
            cli.resolution,
            Some(max_frames),
            focus_start,
            focus_end,
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

    // Dispatch to the correct frame extraction engine
    let (mut extracted, meta) = match detail {
        DetailMode::Efficient => {
            eprintln!("[watch2] engine: keyframe (efficient mode)");
            frames::extract_keyframes(
                vp,
                frames_dir,
                cli.resolution,
                max_frames,
                focus_start,
                focus_end,
                !cli.no_dedup,
            )?
        }
        DetailMode::Balanced => {
            eprintln!(
                "[watch2] engine: scene-or-uniform ({:.2} fps, cap: {})",
                fps, max_frames
            );
            frames::extract_scene_or_uniform(
                vp,
                frames_dir,
                fps,
                max_frames,
                cli.resolution,
                max_frames,
                focus_start,
                focus_end,
                !cli.no_dedup,
            )?
        }
        DetailMode::TokenBurner => {
            eprintln!(
                "[watch2] engine: two-pass ({:.2} fps, cap: {})",
                fps, max_frames
            );
            frames::extract_two_pass(
                vp,
                frames_dir,
                fps,
                max_frames,
                cli.resolution,
                focus_start,
                focus_end,
                !cli.no_dedup,
            )?
        }
        DetailMode::ScreenshotFirst => {
            if transcript_segments.is_empty() {
                eprintln!("[watch2] warning: no transcript for screenshot-first, falling back to uniform");
                frames::extract_scene_or_uniform(
                    vp,
                    frames_dir,
                    fps,
                    max_frames,
                    cli.resolution,
                    max_frames,
                    focus_start,
                    focus_end,
                    !cli.no_dedup,
                )?
            } else {
                let timestamps: Vec<f64> = transcript_segments.iter().map(|s| s.start).collect();
                eprintln!(
                    "[watch2] engine: screenshot-first ({} transcript segments)",
                    timestamps.len()
                );
                frames::extract_at_timestamps(
                    vp,
                    frames_dir,
                    &timestamps,
                    cli.resolution,
                    Some(max_frames),
                    focus_start,
                    focus_end,
                )?
            }
        }
        DetailMode::Transcript => {
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

    // Merge cue frames
    if !cue_frames.is_empty() {
        extracted.extend(cue_frames);
        extracted.sort_by(|a, b| a.timestamp.partial_cmp(&b.timestamp).unwrap());
    }

    let dropped = meta.deduped_count;
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

    Ok(FrameExtractionResult {
        frames: extracted,
        dropped,
        focused,
        meta,
    })
}

fn run_transcript_moments_phase1(
    cli: &cli::Cli,
    detail: &DetailMode,
    work: &PathBuf,
    transcript_segments: &[crate::output::TranscriptSegment],
    transcript_source: &str,
    dl_result: &download::DownloadResult,
    scene_text: &str,
    fusion_text: &str,
) -> anyhow::Result<WatchReport> {
    eprintln!("[watch2] Phase 1: Generating moment detection prompt...");
    let transcript_text =
        crate::moments::format_transcript_for_analysis(transcript_segments);
    let prompt = crate::moments::generate_prompt(
        &transcript_text,
        &dl_result.info.title,
        dl_result.info.uploader.as_deref().unwrap_or("Unknown"),
        dl_result.info.duration.unwrap_or(0.0),
        cli.max_moments,
        cli.min_moments,
    );
    let prompt_path = work.join("moments_prompt.txt");
    std::fs::write(&prompt_path, &prompt)?;
    eprintln!(
        "[watch2] ✅ Moment prompt written to {}",
        prompt_path.display()
    );
    eprintln!();
    eprintln!("📋 Agent workflow:");
    eprintln!("  1. Send the prompt to an LLM to identify key moments");
    eprintln!("  2. Save the LLM JSON response as an array to:");
    eprintln!("     {}", work.join("key_moments.json").display());
    eprintln!("  3. Re-run this command to extract frames at those moments");
    eprintln!();

    // Generate fused prompt if scene data is available
    if !scene_text.is_empty() {
        let fused_prompt = crate::moments::generate_fused_prompt(
            &transcript_text,
            fusion_text,
            scene_text,
            &dl_result.info.title,
            dl_result.info.uploader.as_deref().unwrap_or("Unknown"),
            dl_result.info.duration.unwrap_or(0.0),
            cli.max_moments,
            cli.min_moments,
        );
        let fused_prompt_path = work.join("fused_moments_prompt.txt");
        std::fs::write(&fused_prompt_path, &fused_prompt)?;
        eprintln!("[watch2] ✅ Fused prompt written to {}", fused_prompt_path.display());
    }

    let title = if dl_result.title.is_empty() || dl_result.title == "Unknown" {
        cli.source.clone()
    } else {
        dl_result.title.clone()
    };

    Ok(WatchReport {
        title,
        source: cli.source.clone(),
        detail: detail.to_string(),
        uploader: dl_result.info.uploader.clone(),
        language: dl_result.info.language.clone(),
        engine: Some("transcript-moments-phase1".into()),
        frames: vec![],
        frames_dropped: 0,
        transcript: transcript_segments.to_vec(),
        transcript_source: transcript_source.to_string(),
        duration: dl_result.info.duration.unwrap_or(0.0),
        working_dir: work.to_string_lossy().to_string(),
        warnings: vec!["Phase 1 complete — waiting for key_moments.json".into()],
        key_moments: None,
        key_moment_stats: None,
        fused_moments: None,
        scene_count: None,
    })
}

async fn run_whisper_fallback(
    cli: &cli::Cli,
    config: &WatchConfig,
    work: &PathBuf,
    video_path: &Option<PathBuf>,
    transcript_segments: &mut Vec<crate::output::TranscriptSegment>,
    transcript_source: &mut String,
) {
    let backend = cli
        .whisper
        .as_ref()
        .map(|b| match b {
            cli::WhisperBackend::Groq => "groq",
            cli::WhisperBackend::Openai => "openai",
        })
        .unwrap_or_else(|| config.best_whisper_backend().unwrap_or("none"));

    if backend == "none" {
        return;
    }

    let api_key = match backend {
        "groq" => config.groq_api_key.as_deref(),
        "openai" => config.openai_api_key.as_deref(),
        _ => None,
    };

    if let (Some(key), Some(vp)) = (api_key, video_path.as_ref()) {
        eprintln!("[watch2] transcribing via {}...", backend);
        match whisper::extract_audio(vp, work) {
            Ok(audio_path) => {
                let provider = whisper::create_provider(backend);
                let result = provider.transcribe(&audio_path, key).await;
                match result {
                    Ok(segs) => {
                        *transcript_segments = segs;
                        *transcript_source = format!("whisper ({})", backend);
                        eprintln!(
                            "[watch2] transcript: {} segments",
                            transcript_segments.len()
                        );
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

fn run_transcript_moments_phase2(
    cli: &cli::Cli,
    _detail: &DetailMode,
    work: &PathBuf,
    frames_dir: &PathBuf,
    download_dir: &PathBuf,
    dl_result: &mut download::DownloadResult,
    video_path: &mut Option<PathBuf>,
    duration: &mut f64,
    frames: &mut Vec<FrameInfo>,
    key_moments_raw: &mut Vec<serde_json::Value>,
    key_moment_stats: &mut Option<crate::output::KeyMomentStats>,
    is_url: bool,
    llm_lang: Option<&str>,
) -> anyhow::Result<()> {
    let moments_path = work.join("key_moments.json");
    eprintln!(
        "[watch2] Phase 2: Processing key moments from {}",
        moments_path.display()
    );

    let data = std::fs::read_to_string(&moments_path)?;
    let mut moments: Vec<crate::moments::KeyMoment> = serde_json::from_str(&data)?;
    eprintln!("[watch2] Loaded {} key moments", moments.len());

    // Extract timestamps from moments
    let timestamps =
        crate::moment_frames::get_timestamps_from_moments(&moments, None);
    eprintln!(
        "[watch2] {} unique timestamps for frame extraction",
        timestamps.len()
    );

    // Download video if needed
    if video_path.is_none() && is_url {
        eprintln!("[watch2] Downloading video for frame extraction...");
        *dl_result = download::download_video(&cli.source, download_dir, cli.cookies, llm_lang)?;
        *video_path = dl_result.video_path.clone();
        if let Some(vp) = video_path.as_ref() {
            if let Ok(meta) = frames::get_metadata(vp) {
                *duration = meta.duration;
            }
        }
    }

    // Extract frames at moment timestamps (uncapped)
    if let Some(vp) = video_path.as_ref() {
        eprintln!(
            "[watch2] Extracting frames at {} moment timestamps...",
            timestamps.len()
        );
        let focus_s = cli.start.as_deref().and_then(|s| parse_time(Some(s)));
        let focus_e = cli.end.as_deref().and_then(|s| parse_time(Some(s)));
        let (extracted, meta) = frames::extract_at_timestamps(
            vp,
            frames_dir,
            &timestamps,
            cli.resolution,
            None, // uncapped
            focus_s,
            focus_e,
        )?;
        eprintln!(
            "[watch2] Extracted {} frames ({} dropped out of window)",
            extracted.len(),
            meta.dropped_out_of_window
        );

        // Link moments to frames
        crate::moment_frames::update_moments_with_frames(&mut moments, &extracted);

        // Update frames for report
        *frames = extracted;

        // Serialize moments for report
        *key_moments_raw = moments
            .iter()
            .map(|m| serde_json::to_value(m).unwrap_or_default())
            .collect();

        // Calculate stats
        let mut by_reason: HashMap<String, usize> = HashMap::new();
        let mut by_priority: HashMap<u32, usize> = HashMap::new();
        for m in key_moments_raw.iter() {
            if let Some(reason) = m.get("reason").and_then(|v| v.as_str()) {
                *by_reason.entry(reason.to_string()).or_insert(0) += 1;
            }
            if let Some(priority) = m.get("priority").and_then(|v| v.as_u64()) {
                *by_priority.entry(priority as u32).or_insert(0) += 1;
            }
        }
        *key_moment_stats = Some(crate::output::KeyMomentStats {
            total: key_moments_raw.len(),
            by_reason,
            by_priority,
        });
    } else {
        eprintln!("[watch2] ⚠️  No video available for frame extraction");
    }

    Ok(())
}

fn cleanup(
    cli: &cli::Cli,
    work: &PathBuf,
    video_path: &std::option::Option<PathBuf>,
    dl_result: &download::DownloadResult,
) {
    if !cli.keep_video {
        if let Some(vp) = video_path {
            if dl_result.downloaded {
                let size_mb = std::fs::metadata(vp)
                    .map(|m| m.len() / (1024 * 1024))
                    .unwrap_or(0);
                std::fs::remove_file(vp).ok();
                if size_mb > 0 {
                    eprintln!("[watch2] cleaned up video ({} MB)", size_mb);
                }
            }
        }
    }

    // Cleanup audio temp files
    let audio_tmp = work.join("audio.mp3");
    if audio_tmp.exists() {
        std::fs::remove_file(&audio_tmp).ok();
    }
}

#[allow(clippy::too_many_arguments)]
fn build_report(
    cli: &cli::Cli,
    detail: &DetailMode,
    work: &PathBuf,
    dl_result: &download::DownloadResult,
    frames: Vec<FrameInfo>,
    frames_dropped: u32,
    frame_meta: &frames::FrameMeta,
    transcript_segments: Vec<crate::output::TranscriptSegment>,
    transcript_source: &str,
    duration: f64,
    focused: bool,
    key_moments_raw: Vec<serde_json::Value>,
    key_moment_stats: Option<crate::output::KeyMomentStats>,
    fused_moments: Vec<crate::fusion::FusedMoment>,
    scene_count: Option<usize>,
) -> WatchReport {
    let mut warnings = Vec::new();

    // Token-burner with too many frames
    if *detail == DetailMode::TokenBurner && frames.len() > 250 {
        warnings.push(format!(
            "{} frames selected. This may use a large number of image tokens.",
            frames.len()
        ));
    }

    // Long video with sparse coverage
    if !focused
        && duration > 600.0
        && *detail != DetailMode::Transcript
        && *detail != DetailMode::TokenBurner
    {
        warnings.push(format!(
            "This is a {:.0}-minute video. Frame coverage is sparse under `{}` detail.",
            duration / 60.0,
            detail
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

    WatchReport {
        title,
        source: cli.source.clone(),
        detail: detail.to_string(),
        uploader: dl_result.info.uploader.clone(),
        language: dl_result.info.language.clone(),
        engine: Some(frame_meta.engine.clone()),
        frames,
        frames_dropped,
        transcript: transcript_segments,
        transcript_source: transcript_source.to_string(),
        duration,
        working_dir: work.to_string_lossy().to_string(),
        warnings,
        key_moments: if key_moments_raw.is_empty() {
            None
        } else {
            Some(key_moments_raw)
        },
        key_moment_stats,
        fused_moments: if fused_moments.is_empty() { None } else { Some(fused_moments) },
        scene_count,
    }
}
