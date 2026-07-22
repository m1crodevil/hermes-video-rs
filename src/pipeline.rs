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

/// Linear pipeline — no mode branching.
pub async fn run(ctx: PipelineContext) -> anyhow::Result<WatchReport> {
    let PipelineContext {
        cli, config, max_frames: _, work, download_dir, frames_dir, start_time: _, mut cache,
    } = ctx;

    // ── 1. Materialize resources (download + scene detect) ────────────
    let is_url = download::is_url(&cli.source);
    let (dl_result, mut scene_boundaries) = if is_url {
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

    // ── 2. Parse transcript ───────────────────────────────────────────
    let mut transcript_segments: Vec<crate::output::TranscriptSegment> = Vec::new();
    let mut transcript_source = String::from("none");
    if let Some(ref sub_path) = dl_result.subtitle_path {
        match transcript::parse_subtitle_file(sub_path) {
            Ok(segs) => { transcript_segments = segs; transcript_source = "captions".into(); }
            Err(e) => eprintln!("[watch2] subtitle parse error: {}", e),
        }
    }

    // ── 3. LLM language detection (if URL) ────────────────────────────
    let _llm_lang = if is_url { detect_language(&cli.source, &dl_result, &config, &mut cache).await } else { None };

    // ── 4. Whisper fallback ───────────────────────────────────────────
    if transcript_segments.is_empty() && !cli.no_whisper {
        run_whisper_fallback(&config, &work, &video_path, &mut transcript_segments, &mut transcript_source).await;
    }
    if transcript_segments.is_empty() {
        if cli.no_whisper { eprintln!("⚠️  No subtitles found (--no-whisper skipped whisper)"); }
        else if !config.has_whisper_key() { eprintln!("⚠️  No subtitles found. Set GROQ_API_KEY or OPENAI_API_KEY."); }
    }

    // ── 5. Check key_moments.json → extract frames or generate prompt ──
    let moments_path = work.join("key_moments.json");
    let mut key_moments_raw: Vec<serde_json::Value> = Vec::new();
    let mut key_moment_stats: Option<crate::output::KeyMomentStats> = None;
    let mut frame_vec: Vec<FrameInfo> = Vec::new();
    let mut frame_meta = empty_frame_meta();

    if moments_path.exists() {
        // ── RE-RUN: extract frames at moment timestamps ───────────────
        let mut moments: Vec<crate::moments::KeyMoment> =
            serde_json::from_str(&std::fs::read_to_string(&moments_path)?)?;
        eprintln!("[watch2] {} key moments loaded", moments.len());

        let timestamps = crate::moment_frames::get_timestamps_from_moments(&moments, None);
        eprintln!("[watch2] {} timestamps for extraction", timestamps.len());

        if let Some(ref vp) = video_path {
            scene_boundaries = detect_scenes(vp, duration);
            let (extracted, meta) = frames::extract_at_timestamps(
                vp, &frames_dir, &timestamps, cli.resolution, None, None, None,
            )?;
            crate::moment_frames::update_moments_with_frames(&mut moments, &extracted);
            frame_vec = extracted;
            frame_meta = meta;
            key_moments_raw = moments.iter()
                .map(|m| serde_json::to_value(m).unwrap_or_default())
                .collect();
            key_moment_stats = Some(build_moment_stats(&key_moments_raw));
        } else {
            eprintln!("[watch2] ⚠️  No video available for frame extraction");
        }
    } else {
        // ── FIRST RUN: generate moments prompt, exit ──────────────────
        eprintln!("[watch2] no key_moments.json — generating prompt (Phase 1)");
        if !transcript_segments.is_empty() {
            let text = crate::moments::format_transcript_for_analysis(&transcript_segments);
            let prompt = crate::moments::generate_prompt(
                &text, &dl_result.info.title,
                dl_result.info.uploader.as_deref().unwrap_or("Unknown"),
                dl_result.info.duration.unwrap_or(0.0), 50, None,
            );
            let path = work.join("moments_prompt.txt");
            std::fs::write(&path, &prompt)?;
            eprintln!("[watch2] moments prompt → {}", path.display());
        }

        if !scene_boundaries.is_empty() {
            let _ = crate::scene_detect::write_scene_scores(
                &scene_boundaries, &[], duration, 30.0, 0, &work.join("scene_scores.json"),
            );
        }
        cleanup_audio(&work);
        return Ok(build_report(&cli, &work, &dl_result, vec![], 0, &frame_meta,
            transcript_segments, &transcript_source, duration, false,
            key_moments_raw, key_moment_stats, Some(scene_boundaries.len()), scene_boundaries, None));
    }

    // ── 6. Cleanup ────────────────────────────────────────────────────
    cleanup(&cli, &work, &video_path);

    // ── 7. Build report ───────────────────────────────────────────────
    let scene_count = if scene_boundaries.is_empty() { None } else { Some(scene_boundaries.len()) };
    Ok(build_report(&cli, &work, &dl_result, frame_vec, frame_meta.deduped_count,
        &frame_meta, transcript_segments, &transcript_source, duration, false,
        key_moments_raw, key_moment_stats, scene_count, scene_boundaries, None))
}

// ─── Helpers ──────────────────────────────────────────────────────────────

fn empty_frame_meta() -> frames::FrameMeta {
    frames::FrameMeta {
        engine: "none".into(), candidate_count: 0, selected_count: 0,
        deduped_count: 0, fallback: false, dropped_out_of_window: 0,
    }
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
    for attempt in 1..=3u32 {
        eprintln!("[watch2] downloading (attempt {}/3)...", attempt);
        match crate::download::download_video(source, download_dir, use_cookies, None) {
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
    cleanup_audio(work);
}

fn cleanup_audio(work: &PathBuf) {
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
