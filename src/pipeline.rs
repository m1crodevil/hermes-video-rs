use crate::cache::VideoCache;
use crate::cli;
use crate::config::WatchConfig;
use crate::download;
use crate::frames;
use crate::output::{FrameInfo, WatchReport};
use crate::timestamp;
use crate::transcript;
use crate::whisper;
use std::path::{Path, PathBuf};

pub struct PipelineContext {
    pub cli: cli::Cli,
    pub config: WatchConfig,
    pub work: PathBuf,
    pub download_dir: PathBuf,
    pub frames_dir: PathBuf,
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

    // ── Step 6: Build the extraction report ──────────────────────────
    let title = if dl_result.title.is_empty() || dl_result.title == "Unknown" {
        cli.source.clone()
    } else {
        dl_result.title.clone()
    };
    let scene_count = (!scene_boundaries.is_empty()).then_some(scene_boundaries.len());
    let report = WatchReport {
        title,
        source: cli.source.clone(),
        uploader: dl_result.info.uploader.clone(),
        language: dl_result.info.language.clone(),
        frames: frame_vec,
        transcript: transcript_segments,
        transcript_source,
        duration,
        working_dir: work.to_string_lossy().into_owned(),
        warnings: frame_warnings(&frame_meta, duration),
        scene_boundaries: (!scene_boundaries.is_empty()).then_some(scene_boundaries),
        scene_count,
    };

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
    work: &Path,
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
    if !no_cache
        && let Some(c) = cache
        && let Some(cached) = c.get_video(source)
        && cached.exists()
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
                let d = download_dir.join(sp.file_name()?.to_string_lossy().to_string());
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
    if !cli.keep_video
        && let Some(vp) = video_path
        && vp.starts_with(work)
        && vp.exists()
    {
        let mb = std::fs::metadata(vp)
            .map(|m| m.len() / (1024 * 1024))
            .unwrap_or(0);
        std::fs::remove_file(vp).ok();
        if mb > 0 {
            eprintln!("[watch2] cleaned up video ({} MB)", mb);
        }
    }
    // Clean up audio artifact
    let p = work.join("audio.mp3");
    if p.exists() {
        std::fs::remove_file(&p).ok();
    }
}

fn frame_warnings(meta: &frames::FrameMeta, duration: f64) -> Vec<String> {
    let mut warnings = Vec::new();
    if duration > 600.0 && meta.selected_count > 0 {
        warnings.push(format!(
            "This is a {:.0}-minute video. Frame coverage may be sparse.",
            duration / 60.0
        ));
    }
    if meta.fallback {
        warnings.push(format!(
            "Used {} fallback ({} candidates, below minimum).",
            meta.engine, meta.candidate_count
        ));
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_frame_meta_has_no_selection() {
        let meta = empty_frame_meta();
        assert_eq!(meta.engine, "none");
        assert_eq!(meta.selected_count, 0);
    }

    #[test]
    fn frame_warning_reports_sparse_long_video() {
        let mut meta = empty_frame_meta();
        meta.selected_count = 1;
        assert_eq!(frame_warnings(&meta, 660.0).len(), 1);
    }
}
