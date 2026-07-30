use crate::cache::VideoCache;
use crate::cli;
use crate::config::WatchConfig;
use crate::download;
use crate::frames;
use crate::output::{FrameInfo, WatchReport};
use crate::timestamp;
use crate::transcript;
use crate::whisper;
use std::path::PathBuf;

pub struct PipelineContext {
    pub cli: cli::Cli,
    pub config: WatchConfig,
    pub work: PathBuf,
    pub download_dir: PathBuf,
    pub frames_dir: PathBuf,
    pub start_time: std::time::Instant,
    pub cache: Option<VideoCache>,
}

/// Single-run pipeline — download, analyse, extract frames, report.
pub async fn run(ctx: PipelineContext) -> anyhow::Result<WatchReport> {
    let PipelineContext {
        cli,
        config,
        work,
        download_dir,
        frames_dir,
        start_time: _,
        mut cache,
    } = ctx;

    // ── Step 1: Download video + subtitle + scene detect ──────────────
    let is_url = download::is_url(&cli.source);
    let (dl_result, scene_boundaries) = if is_url {
        ensure_resources(
            &cli.source,
            &download_dir,
            &mut cache,
            cli.cookies,
            cli.no_cache,
        )?
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
            Ok(segs) => {
                transcript_segments = segs;
                transcript_source = "captions".into();
            }
            Err(e) => eprintln!("[watch2] subtitle parse error: {}", e),
        }
    }

    // ── Step 3: Whisper fallback (if no subtitles) ────────────────────
    if transcript_segments.is_empty() && !cli.no_whisper {
        run_whisper_fallback(
            &config,
            &work,
            &video_path,
            &mut transcript_segments,
            &mut transcript_source,
        )
        .await;
    }

    // ── Step 4: Transcript required check ─────────────────────────────
    if transcript_segments.is_empty() {
        if cli.no_whisper {
            anyhow::bail!(
                "No transcript available. Provide subtitles or remove --no-whisper flag."
            );
        } else if !config.has_whisper_key() {
            anyhow::bail!(
                "No transcript available. Set GROQ_API_KEY or OPENAI_API_KEY for Whisper transcription."
            );
        } else {
            anyhow::bail!(
                "No transcript available. Whisper transcription failed — check API key and video format."
            );
        }
    }

    // ── Step 5: Extract uniform frames ────────────────────────────────
    // ── Step 5: Extract frames ────────────────────────────────────────
    let mut frame_vec: Vec<FrameInfo> = Vec::new();
    let mut frame_meta = empty_frame_meta();
    if let Some(ref vp) = video_path {
        let timestamps = if let Some(ref ts_str) = cli.timestamps {
            // Agent-provided timestamps (moment selection)
            let parsed: Vec<f64> = ts_str
                .split(',')
                .filter_map(|s| timestamp::parse_time(Some(s.trim())))
                .collect();
            eprintln!("[watch2] {} agent-provided timestamps", parsed.len());
            parsed
        } else {
            // No timestamps — skip frame extraction
            eprintln!("[watch2] no --timestamps provided, skipping frame extraction");
            eprintln!(
                "[watch2] agent should read report.json, select moments, run again with --timestamps"
            );
            Vec::new()
        };

        if !timestamps.is_empty() {
            let (extracted, meta) = frames::extract_at_timestamps(
                vp,
                &frames_dir,
                &timestamps,
                cli.resolution,
                None,
                None,
                None,
            )?;
            frame_vec = extracted;
            frame_meta = meta;
        }
    }

    // ── Step 6: Build report ──────────────────────────────────────────
    // No key moments in binary — agent handles moment selection via LLM
    let key_moments_raw: Vec<serde_json::Value> = Vec::new();
    let key_moment_stats = None;
    let scene_count = if scene_boundaries.is_empty() {
        None
    } else {
        Some(scene_boundaries.len())
    };

    let report = build_report(
        &cli,
        &work,
        &dl_result,
        frame_vec,
        frame_meta.deduped_count,
        &frame_meta,
        transcript_segments,
        &transcript_source,
        duration,
        false,
        key_moments_raw,
        key_moment_stats,
        scene_count,
        scene_boundaries,
        None,
    );

    // ── Step 7: Cleanup video (after report is ready) ─────────────────
    cleanup(&cli, &work, &video_path);

    Ok(report)
}

// ─── Helpers ──────────────────────────────────────────────────────────────

fn empty_frame_meta() -> frames::FrameMeta {
    frames::FrameMeta {
        engine: "none".into(),
        candidate_count: 0,
        selected_count: 0,
        deduped_count: 0,
        fallback: false,
        dropped_out_of_window: 0,
    }
}

fn detect_scenes(vp: &std::path::Path, duration: f64) -> Vec<crate::scene_detect::SceneBoundary> {
    match crate::scene_detect::detect(vp, 30.0, duration) {
        Ok(r) => {
            eprintln!(
                "[watch2] {} scenes ({:?})",
                r.total_scenes(),
                std::time::Duration::from_millis(r.detection_time_ms)
            );
            r.boundaries
        }
        Err(e) => {
            eprintln!("[watch2] scene detection failed: {}", e);
            vec![]
        }
    }
}
/// Quick language detection via yt-dlp metadata (no video download).
/// Returns language code or None if detection fails.
fn detect_language_quick(url: &str, use_cookies: bool) -> Option<String> {
    let url = crate::download::sanitize_url(url);
    let network_opts = crate::download::ytdlp_network_opts(use_cookies);
    let mut args: Vec<&str> = vec![
        "--skip-download",
        "--write-info-json",
        "--print",
        "language",
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

    let lang = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if lang.is_empty() || lang == "NA" {
        None
    } else {
        Some(lang)
    }
}

async fn run_whisper_fallback(
    config: &WatchConfig,
    work: &PathBuf,
    video_path: &Option<PathBuf>,
    segments: &mut Vec<crate::output::TranscriptSegment>,
    source: &mut String,
) {
    let backend = config.best_whisper_backend().unwrap_or("none");
    if backend == "none" {
        return;
    }
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
                Ok(segs) => {
                    *segments = segs;
                    *source = format!("whisper ({})", backend);
                }
                Err(e) => eprintln!("[watch2] whisper error: {}", e),
            }
        }
    }
}

/// Download with cache and retry (3 attempts, exponential backoff).
fn ensure_resources(
    source: &str,
    download_dir: &std::path::Path,
    cache: &mut Option<crate::cache::VideoCache>,
    use_cookies: bool,
    no_cache: bool,
) -> anyhow::Result<(
    crate::download::DownloadResult,
    Vec<crate::scene_detect::SceneBoundary>,
)> {
    std::fs::create_dir_all(download_dir)?;

    // Cache hit
    if !no_cache {
        if let Some(c) = cache {
            if let Some(cached) = c.get_video(source) {
                if cached.exists()
                    && cached
                        .metadata()
                        .map(|m| m.len() > 1_000_000)
                        .unwrap_or(false)
                {
                    eprintln!("[watch2] ✓ video from cache");
                    let dest = download_dir.join("video.mp4");
                    std::fs::copy(&cached, &dest)?;
                    let info = c.get_info(source).unwrap_or_default();
                    let sub = c
                        .get_subtitles(source, &info.language.clone().unwrap_or_default())
                        .and_then(|sp| {
                            let d =
                                download_dir.join(sp.file_name()?.to_string_lossy().to_string());
                            std::fs::copy(&sp, &d).ok()?;
                            Some(d)
                        });
                    let bounds = detect_scenes(&dest, info.duration.unwrap_or(0.0));
                    return Ok((
                        crate::download::DownloadResult {
                            video_path: Some(dest),
                            subtitle_path: sub,
                            title: info.title.clone(),
                            info,
                            downloaded: false,
                        },
                        bounds,
                    ));
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
        match crate::download::download_video(
            source,
            download_dir,
            use_cookies,
            None,
            detected_lang.as_deref(),
        ) {
            Ok(result) => {
                // Cache the result
                if let Some(c) = cache {
                    if let Some(ref vp) = result.video_path {
                        let _ = c.store_video(source, vp);
                    }
                    if let Some(ref sp) = result.subtitle_path {
                        let _ = c.store_subtitles(
                            source,
                            &result.info.language.clone().unwrap_or_default(),
                            sp,
                        );
                    }
                    let _ = c.store_info(source, &result.info);
                }
                let bounds = if let Some(ref vp) = result.video_path {
                    detect_scenes(vp, result.info.duration.unwrap_or(0.0))
                } else {
                    vec![]
                };
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
    Err(last_err
        .unwrap_or_else(|| crate::error::WatchError::Download("all attempts failed".into()))
        .into())
}

fn cleanup(cli: &cli::Cli, work: &PathBuf, video_path: &Option<PathBuf>) {
    if !cli.keep_video {
        if let Some(vp) = video_path {
            if vp.starts_with(work) && vp.exists() {
                let mb = std::fs::metadata(vp)
                    .map(|m| m.len() / (1024 * 1024))
                    .unwrap_or(0);
                std::fs::remove_file(vp).ok();
                if mb > 0 {
                    eprintln!("[watch2] cleaned up video ({} MB)", mb);
                }
            }
        }
    }
    // Clean up audio artifact
    let p = work.join("audio.mp3");
    if p.exists() {
        std::fs::remove_file(&p).ok();
    }
}

#[allow(clippy::too_many_arguments)]
fn build_report(
    cli: &cli::Cli,
    work: &PathBuf,
    dl: &download::DownloadResult,
    frames: Vec<FrameInfo>,
    frames_dropped: u32,
    meta: &frames::FrameMeta,
    transcript: Vec<crate::output::TranscriptSegment>,
    tsrc: &str,
    duration: f64,
    focused: bool,
    moments: Vec<serde_json::Value>,
    moment_stats: Option<crate::output::KeyMomentStats>,
    scene_count: Option<usize>,
    scenes: Vec<crate::scene_detect::SceneBoundary>,
    scene_scores_path: Option<String>,
) -> WatchReport {
    let mut warnings = Vec::new();
    if !focused && duration > 600.0 && !frames.is_empty() {
        warnings.push(format!(
            "This is a {:.0}-minute video. Frame coverage may be sparse.",
            duration / 60.0
        ));
    }
    if transcript.is_empty() {
        warnings.push("No transcript available.".into());
    }
    if meta.fallback {
        warnings.push(format!(
            "Used {} fallback ({} candidates, below minimum).",
            meta.engine, meta.candidate_count
        ));
    }
    let title = if dl.title.is_empty() || dl.title == "Unknown" {
        cli.source.clone()
    } else {
        dl.title.clone()
    };
    WatchReport {
        title,
        source: cli.source.clone(),
        detail: "balanced".into(),
        uploader: dl.info.uploader.clone(),
        language: dl.info.language.clone(),
        engine: Some(meta.engine.clone()),
        frames,
        frames_dropped,
        transcript,
        transcript_source: tsrc.into(),
        duration,
        working_dir: work.to_string_lossy().to_string(),
        warnings,
        key_moments: if moments.is_empty() {
            None
        } else {
            Some(moments)
        },
        key_moment_stats: moment_stats,
        scene_boundaries: if scenes.is_empty() {
            None
        } else {
            Some(scenes)
        },
        scene_count,
        scene_scores_path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use crate::config::WatchConfig;
    use crate::download::{DownloadResult, VideoInfo};
    use crate::output::{FrameInfo, KeyMomentStats, TranscriptSegment, WatchReport};
    use crate::scene_detect::SceneBoundary;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn make_cli(source: &str) -> Cli {
        Cli {
            source: source.to_string(),
            resolution: 512,
            out_dir: None,
            keep_video: false,
            cookies: false,
            no_whisper: false,
            no_dedup: false,
            output: crate::cli::OutputFormat::Markdown,
            no_cache: false,
            cache_dir: None,
            timestamps: None,
        }
    }

    fn make_config() -> WatchConfig {
        WatchConfig {
            groq_api_key: None,
            openai_api_key: None,
            config_dir: PathBuf::from("/tmp/test-config"),
        }
    }

    fn make_dl_result(title: &str) -> DownloadResult {
        DownloadResult {
            video_path: None,
            subtitle_path: None,
            title: title.to_string(),
            info: VideoInfo {
                title: title.to_string(),
                uploader: Some("TestChannel".into()),
                duration: Some(120.0),
                language: Some("en".into()),
                description: None,
            },
            downloaded: false,
        }
    }

    // ── PipelineContext creation tests ────────────────────────────────

    #[test]
    fn test_pipeline_context_creation_with_valid_inputs() {
        let cli = make_cli("https://example.com/video.mp4");
        let config = make_config();
        let ctx = PipelineContext {
            cli,
            config: config.clone(),
            work: PathBuf::from("/tmp/work"),
            download_dir: PathBuf::from("/tmp/work/downloads"),
            frames_dir: PathBuf::from("/tmp/work/frames"),
            start_time: std::time::Instant::now(),
            cache: None,
        };
        assert_eq!(ctx.cli.source, "https://example.com/video.mp4");
        assert_eq!(ctx.cli.resolution, 512);
        assert_eq!(ctx.work, PathBuf::from("/tmp/work"));
        assert!(ctx.cache.is_none());
        assert!(config.groq_api_key.is_none());
        assert!(config.openai_api_key.is_none());
    }

    #[test]
    fn test_pipeline_context_with_cache() {
        let cli = make_cli("local.mp4");
        let ctx = PipelineContext {
            cli,
            config: make_config(),
            work: PathBuf::from("/tmp/work"),
            download_dir: PathBuf::from("/tmp/work/downloads"),
            frames_dir: PathBuf::from("/tmp/work/frames"),
            start_time: std::time::Instant::now(),
            cache: Some(crate::cache::VideoCache::with_dir(PathBuf::from("/tmp/cache")).unwrap()),
        };
        assert!(ctx.cache.is_some());
    }

    #[test]
    fn test_pipeline_context_local_source() {
        let cli = make_cli("/home/user/videos/local.mp4");
        let ctx = PipelineContext {
            cli,
            config: make_config(),
            work: PathBuf::from("/tmp/work"),
            download_dir: PathBuf::from("/tmp/work/downloads"),
            frames_dir: PathBuf::from("/tmp/work/frames"),
            start_time: std::time::Instant::now(),
            cache: None,
        };
        assert_eq!(ctx.cli.source, "/home/user/videos/local.mp4");
    }

    // ── build_report warning condition tests ──────────────────────────

    #[test]
    fn test_build_report_long_video_with_frames_produces_warning() {
        let cli = make_cli("https://example.com/long.mp4");
        let dl = make_dl_result("Long Video");
        let meta = frames::FrameMeta {
            engine: "scene-or-uniform".into(),
            candidate_count: 10,
            selected_count: 5,
            deduped_count: 5,
            fallback: false,
            dropped_out_of_window: 0,
        };
        let frames = vec![FrameInfo {
            path: "frame_001.jpg".into(),
            timestamp: 10.0,
            reason: "scene change".into(),
            scene_score: Some(0.8),
        }];

        // duration = 660s (>600), frames non-empty, not focused → warning expected
        let report = build_report(
            &cli,
            &PathBuf::from("/tmp/work"),
            &dl,
            frames,
            0,
            &meta,
            vec![],
            "none",
            660.0,
            false,
            vec![],
            None,
            None,
            vec![],
            None,
        );

        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("660-minute video") || w.contains("11-minute video")),
            "Expected '11-minute video' warning but got: {:?}",
            report.warnings
        );
    }

    #[test]
    fn test_build_report_long_video_focused_no_warning() {
        let cli = make_cli("https://example.com/long.mp4");
        let dl = make_dl_result("Long Video Focused");
        let meta = frames::FrameMeta {
            engine: "scene-or-uniform".into(),
            candidate_count: 10,
            selected_count: 5,
            deduped_count: 5,
            fallback: false,
            dropped_out_of_window: 0,
        };
        let frames = vec![FrameInfo {
            path: "frame_001.jpg".into(),
            timestamp: 10.0,
            reason: "scene change".into(),
            scene_score: Some(0.8),
        }];

        // focused=true suppresses the long-video warning
        let report = build_report(
            &cli,
            &PathBuf::from("/tmp/work"),
            &dl,
            frames,
            0,
            &meta,
            vec![],
            "none",
            660.0,
            true, // focused
            vec![],
            None,
            None,
            vec![],
            None,
        );

        assert!(
            !report.warnings.iter().any(|w| w.contains("minute video")),
            "Focused mode should suppress long-video warning, got: {:?}",
            report.warnings
        );
    }

    #[test]
    fn test_build_report_empty_transcript_produces_warning() {
        let cli = make_cli("https://example.com/nosub.mp4");
        let dl = make_dl_result("No Subtitles");
        let meta = frames::FrameMeta {
            engine: "none".into(),
            candidate_count: 0,
            selected_count: 0,
            deduped_count: 0,
            fallback: false,
            dropped_out_of_window: 0,
        };

        let report = build_report(
            &cli,
            &PathBuf::from("/tmp/work"),
            &dl,
            vec![],
            0,
            &meta,
            vec![], // empty transcript
            "none",
            30.0,
            false,
            vec![],
            None,
            None,
            vec![],
            None,
        );

        assert!(
            report.warnings.iter().any(|w| w.contains("No transcript")),
            "Expected 'No transcript' warning, got: {:?}",
            report.warnings
        );
    }

    #[test]
    fn test_build_report_with_transcript_no_warning() {
        let cli = make_cli("https://example.com/sub.mp4");
        let dl = make_dl_result("With Subtitles");
        let meta = frames::FrameMeta {
            engine: "none".into(),
            candidate_count: 0,
            selected_count: 0,
            deduped_count: 0,
            fallback: false,
            dropped_out_of_window: 0,
        };
        let transcript = vec![TranscriptSegment {
            start: 0.0,
            end: 5.0,
            text: "Hello world".into(),
            words: None,
        }];

        let report = build_report(
            &cli,
            &PathBuf::from("/tmp/work"),
            &dl,
            vec![],
            0,
            &meta,
            transcript,
            "captions",
            30.0,
            false,
            vec![],
            None,
            None,
            vec![],
            None,
        );

        assert!(
            !report.warnings.iter().any(|w| w.contains("No transcript")),
            "Should have no 'No transcript' warning, got: {:?}",
            report.warnings
        );
    }

    #[test]
    fn test_build_report_fallback_engine_produces_warning() {
        let cli = make_cli("https://example.com/fallback.mp4");
        let dl = make_dl_result("Fallback Video");
        let meta = frames::FrameMeta {
            engine: "uniform".into(),
            candidate_count: 2,
            selected_count: 2,
            deduped_count: 2,
            fallback: true,
            dropped_out_of_window: 0,
        };
        let transcript = vec![TranscriptSegment {
            start: 0.0,
            end: 5.0,
            text: "Test".into(),
            words: None,
        }];

        let report = build_report(
            &cli,
            &PathBuf::from("/tmp/work"),
            &dl,
            vec![],
            0,
            &meta,
            transcript,
            "captions",
            30.0,
            false,
            vec![],
            None,
            None,
            vec![],
            None,
        );

        assert!(
            report.warnings.iter().any(|w| w.contains("fallback")),
            "Expected 'fallback' warning, got: {:?}",
            report.warnings
        );
    }

    #[test]
    fn test_build_report_all_warnings_combined() {
        let cli = make_cli("https://example.com/messy.mp4");
        let dl = make_dl_result("Unknown");
        let meta = frames::FrameMeta {
            engine: "uniform".into(),
            candidate_count: 1,
            selected_count: 1,
            deduped_count: 1,
            fallback: true,
            dropped_out_of_window: 0,
        };
        let frames = vec![FrameInfo {
            path: "frame.jpg".into(),
            timestamp: 10.0,
            reason: "fallback".into(),
            scene_score: None,
        }];

        // Long video + empty transcript + fallback
        let report = build_report(
            &cli,
            &PathBuf::from("/tmp/work"),
            &dl,
            frames,
            0,
            &meta,
            vec![],
            "none",
            900.0, // 15 minutes
            false,
            vec![],
            None,
            None,
            vec![],
            None,
        );

        assert!(
            report.warnings.len() >= 3,
            "Expected at least 3 warnings, got {}: {:?}",
            report.warnings.len(),
            report.warnings
        );
        assert!(report.warnings.iter().any(|w| w.contains("minute video")));
        assert!(report.warnings.iter().any(|w| w.contains("No transcript")));
        assert!(report.warnings.iter().any(|w| w.contains("fallback")));
    }

    // ── build_report metadata tests ───────────────────────────────────

    #[test]
    fn test_build_report_uses_title_from_dl_result() {
        let cli = make_cli("https://example.com/video");
        let dl = make_dl_result("My Great Video");
        let meta = empty_frame_meta();

        let report = build_report(
            &cli,
            &PathBuf::from("/tmp/work"),
            &dl,
            vec![],
            0,
            &meta,
            vec![TranscriptSegment {
                start: 0.0,
                end: 1.0,
                text: "hi".into(),
                words: None,
            }],
            "captions",
            60.0,
            false,
            vec![],
            None,
            None,
            vec![],
            None,
        );

        assert_eq!(report.title, "My Great Video");
        assert_eq!(report.source, "https://example.com/video");
        assert_eq!(report.uploader, Some("TestChannel".into()));
        assert_eq!(report.language, Some("en".into()));
        assert_eq!(report.transcript_source, "captions");
        assert_eq!(report.duration, 60.0);
        assert_eq!(report.detail, "balanced");
    }

    #[test]
    fn test_build_report_falls_back_to_source_when_title_unknown() {
        let cli = make_cli("https://example.com/video");
        let dl = make_dl_result("Unknown");
        let meta = empty_frame_meta();

        let report = build_report(
            &cli,
            &PathBuf::from("/tmp/work"),
            &dl,
            vec![],
            0,
            &meta,
            vec![TranscriptSegment {
                start: 0.0,
                end: 1.0,
                text: "hi".into(),
                words: None,
            }],
            "captions",
            30.0,
            false,
            vec![],
            None,
            None,
            vec![],
            None,
        );

        // When title is "Unknown", falls back to cli.source
        assert_eq!(report.title, "https://example.com/video");
    }

    #[test]
    fn test_build_report_falls_back_to_source_when_title_empty() {
        let cli = make_cli("local.mp4");
        let mut dl = make_dl_result("");
        dl.title = String::new();
        let meta = empty_frame_meta();

        let report = build_report(
            &cli,
            &PathBuf::from("/tmp/work"),
            &dl,
            vec![],
            0,
            &meta,
            vec![TranscriptSegment {
                start: 0.0,
                end: 1.0,
                text: "hi".into(),
                words: None,
            }],
            "captions",
            30.0,
            false,
            vec![],
            None,
            None,
            vec![],
            None,
        );

        assert_eq!(report.title, "local.mp4");
    }

    #[test]
    fn test_build_report_key_moments_none_when_empty() {
        let cli = make_cli("test.mp4");
        let dl = make_dl_result("Test");
        let meta = empty_frame_meta();

        let report = build_report(
            &cli,
            &PathBuf::from("/tmp/work"),
            &dl,
            vec![],
            0,
            &meta,
            vec![TranscriptSegment {
                start: 0.0,
                end: 1.0,
                text: "hi".into(),
                words: None,
            }],
            "captions",
            30.0,
            false,
            vec![], // empty moments
            None,
            Some(5),
            vec![],
            None,
        );

        assert!(report.key_moments.is_none());
    }

    #[test]
    fn test_build_report_key_moments_some_when_present() {
        let cli = make_cli("test.mp4");
        let dl = make_dl_result("Test");
        let meta = empty_frame_meta();

        let moments = vec![
            serde_json::json!({"timestamp": 10.0, "word": "intro", "reason": "opening", "priority": 1}),
        ];

        let report = build_report(
            &cli,
            &PathBuf::from("/tmp/work"),
            &dl,
            vec![],
            0,
            &meta,
            vec![TranscriptSegment {
                start: 0.0,
                end: 1.0,
                text: "hi".into(),
                words: None,
            }],
            "captions",
            30.0,
            false,
            moments,
            Some(KeyMomentStats {
                total: 1,
                by_reason: HashMap::new(),
                by_priority: HashMap::new(),
            }),
            Some(3),
            vec![],
            None,
        );

        assert!(report.key_moments.is_some());
        assert_eq!(report.key_moments.as_ref().unwrap().len(), 1);
        assert!(report.key_moment_stats.is_some());
    }

    #[test]
    fn test_build_report_scene_count_none_when_no_scenes() {
        let cli = make_cli("test.mp4");
        let dl = make_dl_result("Test");
        let meta = empty_frame_meta();

        let report = build_report(
            &cli,
            &PathBuf::from("/tmp/work"),
            &dl,
            vec![],
            0,
            &meta,
            vec![TranscriptSegment {
                start: 0.0,
                end: 1.0,
                text: "hi".into(),
                words: None,
            }],
            "captions",
            30.0,
            false,
            vec![],
            None,
            None,
            vec![],
            None,
        );

        assert!(report.scene_count.is_none());
        assert!(report.scene_boundaries.is_none());
    }

    #[test]
    fn test_build_report_scene_count_some_when_scenes_present() {
        let cli = make_cli("test.mp4");
        let dl = make_dl_result("Test");
        let meta = empty_frame_meta();
        let scenes = vec![
            SceneBoundary::new(0.0, 5.0, 30.0, 0, 150),
            SceneBoundary::new(5.0, 10.0, 30.0, 150, 300),
            SceneBoundary::new(10.0, 15.0, 30.0, 300, 450),
        ];

        let report = build_report(
            &cli,
            &PathBuf::from("/tmp/work"),
            &dl,
            vec![],
            0,
            &meta,
            vec![TranscriptSegment {
                start: 0.0,
                end: 1.0,
                text: "hi".into(),
                words: None,
            }],
            "captions",
            30.0,
            false,
            vec![],
            None,
            Some(3),
            scenes,
            None,
        );

        assert_eq!(report.scene_count, Some(3));
        assert!(report.scene_boundaries.is_some());
        assert_eq!(report.scene_boundaries.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn test_build_report_frames_dropped_count() {
        let cli = make_cli("test.mp4");
        let dl = make_dl_result("Test");
        let meta = empty_frame_meta();

        let report = build_report(
            &cli,
            &PathBuf::from("/tmp/work"),
            &dl,
            vec![],
            7, // 7 frames dropped
            &meta,
            vec![TranscriptSegment {
                start: 0.0,
                end: 1.0,
                text: "hi".into(),
                words: None,
            }],
            "captions",
            30.0,
            false,
            vec![],
            None,
            None,
            vec![],
            None,
        );

        assert_eq!(report.frames_dropped, 7);
    }

    // ── Report output format tests ────────────────────────────────────

    fn make_full_report() -> WatchReport {
        WatchReport {
            title: "Pipeline Test Video".into(),
            source: "https://example.com/test".into(),
            detail: "balanced".into(),
            uploader: Some("TestCreator".into()),
            language: Some("en".into()),
            engine: Some("scene-or-uniform".into()),
            frames: vec![
                FrameInfo {
                    path: "frames/frame_001.jpg".into(),
                    timestamp: 10.5,
                    reason: "scene change".into(),
                    scene_score: Some(0.85),
                },
                FrameInfo {
                    path: "frames/frame_002.jpg".into(),
                    timestamp: 25.0,
                    reason: "uniform sample".into(),
                    scene_score: None,
                },
            ],
            frames_dropped: 3,
            transcript: vec![
                TranscriptSegment {
                    start: 0.0,
                    end: 5.0,
                    text: "Welcome to the video".into(),
                    words: None,
                },
                TranscriptSegment {
                    start: 5.0,
                    end: 10.0,
                    text: "Let's get started".into(),
                    words: None,
                },
            ],
            transcript_source: "captions".into(),
            duration: 120.5,
            working_dir: "/tmp/pipeline-test".into(),
            warnings: vec!["Test warning message".into()],
            key_moments: Some(vec![
                serde_json::json!({"timestamp": 10.5, "word": "welcome", "reason": "opening", "priority": 1}),
            ]),
            key_moment_stats: Some(KeyMomentStats {
                total: 1,
                by_reason: HashMap::new(),
                by_priority: HashMap::new(),
            }),
            scene_boundaries: Some(vec![SceneBoundary::new(0.0, 10.0, 30.0, 0, 300)]),
            scene_count: Some(1),
            scene_scores_path: Some("/tmp/scores.json".into()),
        }
    }

    #[test]
    fn test_report_markdown_header() {
        let report = make_full_report();
        let md = report.to_markdown();
        assert!(md.starts_with("# Pipeline Test Video\n\n"));
        assert!(md.contains("**Source:** https://example.com/test"));
        assert!(md.contains("**Detail:** balanced"));
        assert!(md.contains("**Uploader:** TestCreator"));
        assert!(md.contains("**Language:** en"));
        assert!(md.contains("**Engine:** scene-or-uniform"));
    }

    #[test]
    fn test_report_markdown_frames_section() {
        let report = make_full_report();
        let md = report.to_markdown();
        assert!(md.contains("## Frames (2 total, 3 dropped)"));
        assert!(md.contains("frames/frame_001.jpg"));
        assert!(md.contains("frames/frame_002.jpg"));
        assert!(md.contains("scene change"));
        assert!(md.contains("uniform sample"));
    }

    #[test]
    fn test_report_markdown_transcript_section() {
        let report = make_full_report();
        let md = report.to_markdown();
        assert!(md.contains("## Transcript (captions)"));
        assert!(md.contains("Welcome to the video"));
        assert!(md.contains("Let's get started"));
    }

    #[test]
    fn test_report_markdown_key_moments_section() {
        let report = make_full_report();
        let md = report.to_markdown();
        assert!(md.contains("## Key Moments (1)"));
        assert!(md.contains("welcome"));
        assert!(md.contains("opening"));
    }

    #[test]
    fn test_report_markdown_warnings_section() {
        let report = make_full_report();
        let md = report.to_markdown();
        assert!(md.contains("## Warnings"));
        assert!(md.contains("⚠️ Test warning message"));
    }

    #[test]
    fn test_report_markdown_scene_scores_path() {
        let report = make_full_report();
        let md = report.to_markdown();
        assert!(md.contains("**Scene Scores:** `/tmp/scores.json`"));
    }

    #[test]
    fn test_report_markdown_no_frames_no_transcript_fallback() {
        let report = WatchReport {
            title: "Empty".into(),
            source: "none".into(),
            detail: "balanced".into(),
            uploader: None,
            language: None,
            engine: None,
            frames: vec![],
            frames_dropped: 0,
            transcript: vec![],
            transcript_source: "none".into(),
            duration: 0.0,
            working_dir: "/tmp".into(),
            warnings: vec![],
            key_moments: None,
            key_moment_stats: None,
            scene_boundaries: None,
            scene_count: None,
            scene_scores_path: None,
        };
        let md = report.to_markdown();
        assert!(md.contains("*No frames or transcript available.*"));
    }

    #[test]
    fn test_report_json_serialization() {
        let report = make_full_report();
        let json_str = report.to_json();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("valid JSON");

        assert_eq!(parsed["title"].as_str().unwrap(), "Pipeline Test Video");
        assert_eq!(
            parsed["source"].as_str().unwrap(),
            "https://example.com/test"
        );
        assert_eq!(parsed["detail"].as_str().unwrap(), "balanced");
        assert_eq!(parsed["uploader"].as_str().unwrap(), "TestCreator");
        assert_eq!(parsed["language"].as_str().unwrap(), "en");
        assert_eq!(parsed["engine"].as_str().unwrap(), "scene-or-uniform");
        assert_eq!(parsed["duration"].as_f64().unwrap(), 120.5);
        assert_eq!(parsed["transcript_source"].as_str().unwrap(), "captions");
        assert_eq!(
            parsed["working_dir"].as_str().unwrap(),
            "/tmp/pipeline-test"
        );
        assert_eq!(parsed["scene_count"].as_u64().unwrap(), 1);
        assert_eq!(
            parsed["scene_scores_path"].as_str().unwrap(),
            "/tmp/scores.json"
        );
    }

    #[test]
    fn test_report_json_frames_array() {
        let report = make_full_report();
        let parsed: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();

        let frames = parsed["frames"].as_array().unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0]["path"].as_str().unwrap(), "frames/frame_001.jpg");
        assert_eq!(frames[0]["timestamp"].as_f64().unwrap(), 10.5);
        assert_eq!(frames[0]["reason"].as_str().unwrap(), "scene change");
        assert_eq!(frames[0]["scene_score"].as_f64().unwrap(), 0.85);
        // Second frame has no scene_score (None → omitted)
        assert!(frames[1].get("scene_score").is_none());
    }

    #[test]
    fn test_report_json_transcript_array() {
        let report = make_full_report();
        let parsed: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();

        let transcript = parsed["transcript"].as_array().unwrap();
        assert_eq!(transcript.len(), 2);
        assert_eq!(
            transcript[0]["text"].as_str().unwrap(),
            "Welcome to the video"
        );
        assert_eq!(transcript[0]["start"].as_f64().unwrap(), 0.0);
        assert_eq!(transcript[0]["end"].as_f64().unwrap(), 5.0);
    }

    #[test]
    fn test_report_json_warnings_array() {
        let report = make_full_report();
        let parsed: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();

        let warnings = parsed["warnings"].as_array().unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].as_str().unwrap(), "Test warning message");
    }

    #[test]
    fn test_report_json_empty_warnings_omitted() {
        let report = WatchReport {
            title: "No Warnings".into(),
            source: "s".into(),
            detail: "d".into(),
            uploader: None,
            language: None,
            engine: None,
            frames: vec![],
            frames_dropped: 0,
            transcript: vec![],
            transcript_source: "none".into(),
            duration: 0.0,
            working_dir: "/tmp".into(),
            warnings: vec![],
            key_moments: None,
            key_moment_stats: None,
            scene_boundaries: None,
            scene_count: None,
            scene_scores_path: None,
        };
        let parsed: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();
        // Empty warnings Vec is skipped by skip_serializing_if = "Vec::is_empty"
        assert!(parsed.get("warnings").is_none());
    }

    #[test]
    fn test_report_json_key_moments_structure() {
        let report = make_full_report();
        let parsed: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();

        let moments = parsed["key_moments"].as_array().unwrap();
        assert_eq!(moments.len(), 1);
        assert_eq!(moments[0]["timestamp"].as_f64().unwrap(), 10.5);
        assert_eq!(moments[0]["word"].as_str().unwrap(), "welcome");
        assert_eq!(moments[0]["reason"].as_str().unwrap(), "opening");
        assert_eq!(moments[0]["priority"].as_u64().unwrap(), 1);
    }

    #[test]
    fn test_report_json_scene_boundaries_structure() {
        let report = make_full_report();
        let parsed: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();

        let scenes = parsed["scene_boundaries"].as_array().unwrap();
        assert_eq!(scenes.len(), 1);
        assert_eq!(scenes[0]["start_sec"].as_f64().unwrap(), 0.0);
        assert_eq!(scenes[0]["end_sec"].as_f64().unwrap(), 10.0);
    }

    // ── empty_frame_meta tests ────────────────────────────────────────

    #[test]
    fn test_empty_frame_meta_defaults() {
        let meta = empty_frame_meta();
        assert_eq!(meta.engine, "none");
        assert_eq!(meta.candidate_count, 0);
        assert_eq!(meta.selected_count, 0);
        assert_eq!(meta.deduped_count, 0);
        assert!(!meta.fallback);
        assert_eq!(meta.dropped_out_of_window, 0);
    }
}
