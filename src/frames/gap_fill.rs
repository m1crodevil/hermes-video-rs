use crate::error::Result;
use crate::output::FrameInfo;
use super::scale_filter;

pub fn fill_gaps_with_uniform(
    scene_frames: &[FrameInfo],
    video_path: &std::path::Path,
    out_dir: &std::path::Path,
    resolution: u32,
    duration: f64,
    target_frames: usize,
) -> Result<Vec<FrameInfo>> {
    if scene_frames.len() < 2 {
        return Ok(scene_frames.to_vec());
    }

    let expected_interval = duration / target_frames as f64;
    let fill_threshold = 120.0_f64.min(expected_interval * 2.0);
    let scale = scale_filter(resolution);

    let mut fill_frames: Vec<FrameInfo> = Vec::new();
    let mut fill_counter = 0u32;

    for pair in scene_frames.windows(2) {
        let gap = pair[1].timestamp - pair[0].timestamp;
        if gap > fill_threshold {
            let n_fill = ((gap / fill_threshold).floor() as usize).min(5);
            if n_fill == 0 {
                continue;
            }

            for i in 1..=n_fill {
                let t = pair[0].timestamp + gap * (i as f64) / (n_fill as f64 + 1.0);
                let out_path = out_dir.join(format!("fill_{:04}.jpg", fill_counter));

                let status = std::process::Command::new("ffmpeg")
                    .args([
                        "-hide_banner",
                        "-loglevel",
                        "error",
                        "-ss",
                        &format!("{t:.3}"),
                        "-i",
                        video_path.to_str().unwrap_or(""),
                        "-frames:v",
                        "1",
                        "-vf",
                        &scale,
                        "-q:v",
                        "4",
                        out_path.to_str().unwrap_or(""),
                    ])
                    .status();

                if status.map(|s| s.success()).unwrap_or(false) {
                    fill_frames.push(FrameInfo {
                        path: out_path.to_string_lossy().to_string(),
                        timestamp: t,
                        reason: "gap-fill".to_string(),
                    });
                    fill_counter += 1;
                } else {
                    eprintln!(
                        "[watch2] warning: gap-fill frame at {t:.2}s failed, skipping"
                    );
                }
            }
        }
    }

    let mut all: Vec<FrameInfo> = scene_frames
        .iter()
        .chain(fill_frames.iter())
        .cloned()
        .collect();
    all.sort_by(|a, b| a.timestamp.partial_cmp(&b.timestamp).unwrap());

    Ok(all)
}
