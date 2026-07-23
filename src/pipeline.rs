use crate::cli;
use crate::cache::VideoCache;
use crate::config::WatchConfig;
use crate::download;
use crate::frames;
use crate::output::{FrameInfo, WatchReport};
use crate::transcript;
use crate::whisper;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct PipelineContext {
    pub cli: cli::Cli,
    pub config: WatchConfig,
    pub max_frames: u32,
    pub work: PathBuf,
    pub download_dir: PathBuf,
    pub frames_dir: PathBuf,
    pub start_time: std::time::Instant,
    pub cache: Option<VideoCache>,
}

/// Single-run pipeline — download, analyse, extract frames, report.
pub async fn run(ctx: PipelineContext) -> anyhow::Result<WatchReport> {
    let PipelineContext {
        cli, config, max_frames, work, download_dir, frames_dir, start_time: _, mut cache,
    } = ctx;

    // ── Step 1: Download video + subtitle + scene detect ──────────────
    let is_url = download::is_url(&cli.source);
    let (dl_result, scene_boundaries) = if is_url {
        ensure_resources(&cli.source, &download_dir, &mut cache, cli.cookies, cli.no_cache)?
    } else {
        (download::resolve_local(&cli.source)?, vec![])
    };

    let video_path: Option<PathBuf> = dl_result.video_path.clone();
    let mut duration = 0.0;
    if let Some(ref vp) = video_path {
        match frames::get_metadata(vp) {
            Ok(meta) => duration = meta.duration,
            Err(e) => eprintln!("[watch2] metadata error: {}", e),
        }
    }
    if duration <= 0.0 {
        duration = dl_result.info.duration.unwrap_or(0.0);
    }

    // ── Step 2: Parse transcript ──────────────────────────────────────
    let mut transcript_segments: Vec<crate::output::TranscriptSegment> = Vec::new();
    let mut transcript_source = String::from("none");
    if let Some(ref sub_path) = dl_result.subtitle_path {
        match transcript::parse_subtitle_file(sub_path) {
            Ok(segs) => { transcript_segments = segs; transcript_source = "captions".into(); }
            Err(e) => eprintln!("[watch2] subtitle parse error: {}", e),
        }
    }

    // ── Step 3: Detect language (cache → metadata → LLM) ─────────────
    let _llm_lang = if is_url {
        detect_language(&cli.source, &dl_result, &config, &mut cache).await
    } else {
        None
    };

    // ── Step 4: Whisper fallback (if no subtitles) ────────────────────
    if transcript_segments.is_empty() && !cli.no_whisper {
        run_whisper_fallback(&config, &work, &video_path, &mut transcript_segments, &mut transcript_source).await;
    }
    if transcript_segments.is_empty() {
        if cli.no_whisper { eprintln!("⚠️  No subtitles found (--no-whisper skipped whisper)"); }
        else if !config.has_whisper_key() { eprintln!("⚠️  No subtitles found. Set GROQ_API_KEY or OPENAI_API_KEY."); }
    }

    // ── Step 5: LLM moment selection ──────────────────────────────────
    let mut key_moments: Vec<crate::moments::KeyMoment> = Vec::new();
    if !transcript_segments.is_empty() {
        let text = crate::moments::format_transcript_for_analysis(&transcript_segments);
        let scene_text = format_scene_text(&scene_boundaries);
        match crate::llm::select_moments(
            &text,
            &dl_result.info.title,
            dl_result.info.uploader.as_deref().unwrap_or("Unknown"),
            duration,
            &scene_text,
            &config,
        ).await {
            Some(moments) => {
                eprintln!("[watch2] {} key moments from LLM", moments.len());
                // Save key_moments.json for reference / debugging
                if let Ok(json) = serde_json::to_string_pretty(&moments) {
                    let _ = std::fs::write(work.join("key_moments.json"), &json);
                }
                key_moments = moments;
            }
            None => eprintln!("[watch2] LLM moment selection failed or returned empty"),
        }
    } else {
        eprintln!("[watch2] no transcript — skipping moment selection");
    }

    // ── Step 6: Extract frames ────────────────────────────────────────
    let mut frame_vec: Vec<FrameInfo> = Vec::new();
    let mut frame_meta = empty_frame_meta();

    if !key_moments.is_empty() {
        // Extract at moment timestamps
        let timestamps = crate::moment_frames::get_timestamps_from_moments(&key_moments, None);
        eprintln!("[watch2] {} timestamps for extraction", timestamps.len());

        if let Some(ref vp) = video_path {
            let (extracted, meta) = frames::extract_at_timestamps(
                vp, &frames_dir, &timestamps, cli.resolution, None, None, None,
            )?;
            crate::moment_frames::update_moments_with_frames(&mut key_moments, &extracted);
            frame_vec = extracted;
            frame_meta = meta;
        } else {
            eprintln!("[watch2] ⚠️  No video available for frame extraction");
        }
    } else if video_path.is_some() {
        // Fallback: extract uniform frames across the video
        let timestamps = generate_uniform_timestamps(duration, max_frames);
        if !timestamps.is_empty() {
            eprintln!("[watch2] {} uniform timestamps (fallback)", timestamps.len());
            if let Some(ref vp) = video_path {
                let (extracted, meta) = frames::extract_at_timestamps(
                    vp, &frames_dir, &timestamps, cli.resolution, None, None, None,
                )?;
                frame_vec = extracted;
                frame_meta = meta;
            }
        }
    }

    // ── Step 7: Cleanup video ─────────────────────────────────────────
    cleanup(&cli, &work, &video_path);

    // ── Step 8: Build report ──────────────────────────────────────────
    let key_moments_raw: Vec<serde_json::Value> = key_moments.iter()
        .map(|m| serde_json::to_value(m).unwrap_or_default())
        .collect();
    let key_moment_stats = if key_moments_raw.is_empty() {
        None
    } else {
        Some(build_moment_stats(&key_moments_raw))
    };
    let scene_count = if scene_boundaries.is_empty() { None } else { Some(scene_boundaries.len()) };

    Ok(build_report(
        &cli, &work, &dl_result, frame_vec, frame_meta.deduped_count,
        &frame_meta, transcript_segments, &transcript_source, duration, false,
        key_moments_raw, key_moment_stats, scene_count, scene_boundaries, None,
    ))
}

// ─── Helpers ──────────────────────────────────────────────────────────────

fn empty_frame_meta() -> frames::FrameMeta {
    frames::FrameMeta {
        engine: "none".into(), candidate_count: 0, selected_count: 0,
        deduped_count: 0, fallback: false, dropped_out_of_window: 0,
    }
}

/// Format scene boundaries as human-readable text for the LLM prompt.
fn format_scene_text(boundaries: &[crate::scene_detect::SceneBoundary]) -> String {
    if boundaries.is_empty() {
        return "No scene changes detected.".to_string();
    }
    boundaries.iter().enumerate().map(|(i, b)| {
        format!(
            "Scene {}: {} - {} ({:.1}s)",
            i + 1,
            crate::moments::format_timestamp(b.start_sec),
            crate::moments::format_timestamp(b.end_sec),
            b.duration_sec,
        )
    }).collect::<Vec<_>>().join("\n")
}

/// Generate evenly-spaced timestamps across the video for uniform frame extraction.
fn generate_uniform_timestamps(duration: f64, count: u32) -> Vec<f64> {
    if duration <= 0.0 || count == 0 {
        return Vec::new();
    }
    let n = count.min(20).max(1);
    let step = duration / (n as f64 + 1.0);
    (1..=n).map(|i| step * i as f64).collect()
}

fn detect_scenes(vp: &std::path::Path, duration: f64) -> Vec<crate::scene_detect::SceneBoundary> {
    match crate::scene_detect::detect(vp, 30.0, duration) {
        Ok(r) => {
            eprintln!("[watch2] {} scenes ({:?})", r.total_scenes(),
                std::time::Duration::from_millis(r.detection_time_ms));
            r.boundaries
        }
        Err(e) => { eprintln!("[watch2] scene detection failed: {}", e); vec![] }
    }
}

fn build_moment_stats(raw: &[serde_json::Value]) -> crate::output::KeyMomentStats {
    let mut by_reason: HashMap<String, usize> = HashMap::new();
    let mut by_priority: HashMap<u32, usize> = HashMap::new();
    for m in raw {
        if let Some(r) = m.get("reason").and_then(|v| v.as_str()) {
            *by_reason.entry(r.to_string()).or_insert(0) += 1;
        }
        if let Some(p) = m.get("priority").and_then(|v| v.as_u64()) {
            *by_priority.entry(p as u32).or_insert(0) += 1;
        }
    }
    crate::output::KeyMomentStats { total: raw.len(), by_reason, by_priority }
}

async fn detect_language(
    source: &str, dl: &download::DownloadResult, config: &WatchConfig,
    cache: &mut Option<crate::cache::VideoCache>,
) -> Option<String> {
    // Cache → metadata → LLM
    let cached = cache.as_mut().and_then(|c| c.get_cached_language(source));
    if let Some(ref lang) = cached {
        eprintln!("[watch2] language from cache: {}", lang);
        if let Some(c) = cache { let _ = c.store_language(source, lang); }
        return cached;
    }
    if let Some(ref lang) = dl.info.language {
        eprintln!("[watch2] language from metadata: {}", lang);
        if let Some(c) = cache { let _ = c.store_language(source, lang); }
        return Some(lang.clone());
    }
    let lang = crate::llm::detect_language_llm(&dl.info.title, dl.info.description.as_deref(), config).await;
    if let Some(ref l) = lang {
        eprintln!("[watch2] LLM detected language: {}", l);
        if let Some(c) = cache { let _ = c.store_language(source, l); }
    }
    lang
}

/// Quick language detection via yt-dlp metadata (no video download).
/// Returns language code or None if detection fails.
fn detect_language_quick(url: &str, use_cookies: bool) -> Option<String> {
    let url = crate::download::sanitize_url(url);
    let network_opts = crate::download::ytdlp_network_opts(use_cookies);
    let mut args: Vec<&str> = vec![
        "--skip-download",
        "--write-info-json",
        "--print", "language",
        "--no-playlist",
    ];
    for opt in &network_opts {
        args.push(opt.as_str());
    }
    args.push("--");
    args.push(&url);

    let output = std::process::Command::new("yt-dlp")
        .args(&args)
        .output()
        .ok()?;

    let lang = String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_string();

    if lang.is_empty() || lang == "NA" {
        None
    } else {
        Some(lang)
    }
}

async fn run_whisper_fallback(
    config: &WatchConfig, work: &PathBuf, video_path: &Option<PathBuf>,
    segments: &mut Vec<crate::output::TranscriptSegment>, source: &mut String,
) {
    let backend = config.best_whisper_backend().unwrap_or("none");
    if backend == "none" { return; }
    let key = match backend {
        "groq" => config.groq_api_key.as_deref(),
        "openai" => config.openai_api_key.as_deref(),
        _ => None,
    };
    if let (Some(key), Some(vp)) = (key, video_path.as_ref()) {
        eprintln!("[watch2] transcribing via {}...", backend);
        if let Ok(audio) = whisper::extract_audio(vp, work) {
            let provider = whisper::create_provider(backend);
            match provider.transcribe(&audio, key).await {
                Ok(segs) => { *segments = segs; *source = format!("whisper ({})", backend); }
                Err(e) => eprintln!("[watch2] whisper error: {}", e),
            }
        }
    }
}

/// Download with cache and retry (3 attempts, exponential backoff).
fn ensure_resources(
    source: &str, download_dir: &std::path::Path,
    cache: &mut Option<crate::cache::VideoCache>,
    use_cookies: bool, no_cache: bool,
) -> anyhow::Result<(crate::download::DownloadResult, Vec<crate::scene_detect::SceneBoundary>)> {
    std::fs::create_dir_all(download_dir)?;

    // Cache hit
    if !no_cache {
        if let Some(c) = cache {
            if let Some(cached) = c.get_video(source) {
                if cached.exists() && cached.metadata().map(|m| m.len() > 1_000_000).unwrap_or(false) {
                    eprintln!("[watch2] ✓ video from cache");
                    let dest = download_dir.join("video.mp4");
                    std::fs::copy(&cached, &dest)?;
                    let info = c.get_info(source).unwrap_or_default();
                    let sub = c.get_subtitles(source, &info.language.clone().unwrap_or_default())
                        .and_then(|sp| {
                            let d = download_dir.join(sp.file_name()?.to_string_lossy().to_string());
                            std::fs::copy(&sp, &d).ok()?; Some(d)
                        });
                    let bounds = detect_scenes(&dest, info.duration.unwrap_or(0.0));
                    return Ok((crate::download::DownloadResult {
                        video_path: Some(dest), subtitle_path: sub,
                        title: info.title.clone(), info, downloaded: false,
                    }, bounds));
                }
            }
        }
    }

    // Download with retry
    let mut last_err: Option<crate::error::WatchError> = None;
    // Detect language before download to minimize subtitle requests
    let detected_lang = detect_language_quick(source, use_cookies);
    for attempt in 1..=3u32 {
        eprintln!("[watch2] downloading (attempt {}/3)...", attempt);
        match crate::download::download_video(source, download_dir, use_cookies, None, detected_lang.as_deref()) {
            Ok(result) => {
                // Cache the result
                if let Some(c) = cache {
                    if let Some(ref vp) = result.video_path { let _ = c.store_video(source, vp); }
                    if let Some(ref sp) = result.subtitle_path {
                        let _ = c.store_subtitles(source, &result.info.language.clone().unwrap_or_default(), sp);
                    }
                    let _ = c.store_info(source, &result.info);
                }
                let bounds = if let Some(ref vp) = result.video_path {
                    detect_scenes(vp, result.info.duration.unwrap_or(0.0))
                } else { vec![] };
                return Ok((result, bounds));
            }
            Err(e) => {
                eprintln!("[watch2] ✗ download failed: {}", e);
                last_err = Some(e);
                if attempt < 3 {
                    let delay = std::time::Duration::from_secs(2u64.pow(attempt));
                    std::thread::sleep(delay);
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| crate::error::WatchError::Download("all attempts failed".into())).into())
}

fn cleanup(cli: &cli::Cli, work: &PathBuf, video_path: &Option<PathBuf>) {
    if !cli.keep_video {
        if let Some(vp) = video_path {
            if vp.starts_with(work) && vp.exists() {
                let mb = std::fs::metadata(vp).map(|m| m.len() / (1024 * 1024)).unwrap_or(0);
                std::fs::remove_file(vp).ok();
                if mb > 0 { eprintln!("[watch2] cleaned up video ({} MB)", mb); }
            }
        }
    }
    // Clean up audio artifact
    let p = work.join("audio.mp3");
    if p.exists() { std::fs::remove_file(&p).ok(); }
}

#[allow(clippy::too_many_arguments)]
fn build_report(
    cli: &cli::Cli, work: &PathBuf, dl: &download::DownloadResult,
    frames: Vec<FrameInfo>, frames_dropped: u32, meta: &frames::FrameMeta,
    transcript: Vec<crate::output::TranscriptSegment>, tsrc: &str,
    duration: f64, focused: bool, moments: Vec<serde_json::Value>,
    moment_stats: Option<crate::output::KeyMomentStats>,
    scene_count: Option<usize>, scenes: Vec<crate::scene_detect::SceneBoundary>,
    scene_scores_path: Option<String>,
) -> WatchReport {
    let mut warnings = Vec::new();
    if !focused && duration > 600.0 && !frames.is_empty() {
        warnings.push(format!("This is a {:.0}-minute video. Frame coverage may be sparse.", duration / 60.0));
    }
    if transcript.is_empty() { warnings.push("No transcript available.".into()); }
    if meta.fallback {
        warnings.push(format!("Used {} fallback ({} candidates, below minimum).", meta.engine, meta.candidate_count));
    }
    let title = if dl.title.is_empty() || dl.title == "Unknown" { cli.source.clone() } else { dl.title.clone() };
    WatchReport {
        title, source: cli.source.clone(), detail: "balanced".into(),
        uploader: dl.info.uploader.clone(), language: dl.info.language.clone(),
        engine: Some(meta.engine.clone()), frames, frames_dropped,
        transcript, transcript_source: tsrc.into(), duration,
        working_dir: work.to_string_lossy().to_string(), warnings,
        key_moments: if moments.is_empty() { None } else { Some(moments) },
        key_moment_stats: moment_stats,
        scene_boundaries: if scenes.is_empty() { None } else { Some(scenes) },
        scene_count, scene_scores_path,
    }
}
