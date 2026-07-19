use crate::moments::format_timestamp;
use crate::output::{TranscriptSegment, WordTiming};
use crate::scene_detect::SceneBoundary;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Position of a timestamp within its containing scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScenePosition {
    /// < 1s from scene boundary
    AtCut,
    /// 1-3s after cut
    EarlyScene,
    /// 3-7s into scene
    MidScene,
    /// < 3s before next cut
    LateScene,
}

impl std::fmt::Display for ScenePosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScenePosition::AtCut => write!(f, "AtCut"),
            ScenePosition::EarlyScene => write!(f, "EarlyScene"),
            ScenePosition::MidScene => write!(f, "MidScene"),
            ScenePosition::LateScene => write!(f, "LateScene"),
        }
    }
}

/// Classify where a timestamp falls within a scene.
pub fn classify_position(scene: &SceneBoundary, timestamp: f64) -> ScenePosition {
    let since_start = timestamp - scene.start_sec;
    let until_end = scene.end_sec - timestamp;
    if since_start.abs() < 1.0 {
        ScenePosition::AtCut
    } else if since_start < 3.0 {
        ScenePosition::EarlyScene
    } else if until_end < 3.0 {
        ScenePosition::LateScene
    } else {
        ScenePosition::MidScene
    }
}

/// A fused moment combining scene detection and transcript data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusedMoment {
    pub timestamp: f64,
    pub timestamp_fmt: String,
    pub word: String,
    pub asr_confidence: Option<i32>,
    pub scene_boundary: Option<SceneBoundary>,
    pub is_at_scene_cut: bool,
    pub scene_index: usize,
    pub scene_position: ScenePosition,
    pub reason: String,
    pub priority: u32,
}

/// Find words from transcript spoken near a scene boundary (within tolerance seconds).
/// Checks proximity to both `start_sec` (scene entry cut) and `end_sec` (scene exit cut).
pub fn find_words_near_boundary(
    segments: &[TranscriptSegment],
    scene: &SceneBoundary,
    tolerance: f64,
) -> Vec<WordTiming> {
    let mut results = Vec::new();
    for seg in segments {
        if let Some(ref words) = seg.words {
            for w in words {
                let near_start = (w.start - scene.start_sec).abs() <= tolerance;
                let near_end = (w.start - scene.end_sec).abs() <= tolerance;
                if near_start || near_end {
                    results.push(WordTiming {
                        word: w.word.clone(),
                        start: w.start,
                        confidence: w.confidence,
                    });
                }
            }
        }
    }
    results
}

/// Find the scene boundary containing a given timestamp.
pub fn find_containing_scene(
    scenes: &[SceneBoundary],
    timestamp: f64,
) -> Option<(usize, &SceneBoundary)> {
    scenes
        .iter()
        .enumerate()
        .find(|(_, s)| timestamp >= s.start_sec && timestamp < s.end_sec)
}

/// Fuse scene boundaries and transcript data into FusedMoment candidates.
pub fn fuse_scenes_and_transcript(
    scenes: &[SceneBoundary],
    transcript: &[TranscriptSegment],
    _video_duration: f64,
) -> Vec<FusedMoment> {
    let mut moments: Vec<FusedMoment> = Vec::new();
    let mut seen_keys: HashSet<(String, String)> = HashSet::new();

    // 1. For each scene boundary, find words near it (tolerance 1.0s)
    for (scene_idx, scene) in scenes.iter().enumerate() {
        let nearby_words = find_words_near_boundary(transcript, scene, 1.0);
        for w in nearby_words {
            let key = (
                format!("{:.2}", w.start),
                w.word.to_lowercase(),
            );
            if seen_keys.contains(&key) {
                continue;
            }

            let is_low_conf = w.confidence < 70;
            let priority = if is_low_conf { 1 } else { 3 };
            let reason = if is_low_conf {
                "scene_boundary_low_confidence".to_string()
            } else {
                "scene_boundary".to_string()
            };

            moments.push(FusedMoment {
                timestamp: w.start,
                timestamp_fmt: format_timestamp(w.start),
                word: w.word.clone(),
                asr_confidence: Some(w.confidence),
                scene_boundary: Some(scene.clone()),
                is_at_scene_cut: true,
                scene_index: scene_idx,
                scene_position: classify_position(scene, w.start),
                reason,
                priority,
            });
            seen_keys.insert(key);
        }
    }

    // 2. For each word in transcript with confidence < 70, not already added
    for seg in transcript {
        if let Some(ref words) = seg.words {
            for w in words {
                if w.confidence >= 70 {
                    continue;
                }

                let key = (
                    format!("{:.2}", w.start),
                    w.word.to_lowercase(),
                );
                if seen_keys.contains(&key) {
                    continue;
                }

                let priority = if w.confidence <= 50 { 1 } else { 2 };
                let (scene_idx, scene) = find_containing_scene(scenes, w.start)
                    .unwrap_or((0, &SceneBoundary {
                        start_sec: 0.0,
                        end_sec: f64::INFINITY,
                        duration_sec: f64::INFINITY,
                        frame_start: 0,
                        frame_end: 0,
                    }));

                moments.push(FusedMoment {
                    timestamp: w.start,
                    timestamp_fmt: format_timestamp(w.start),
                    word: w.word.clone(),
                    asr_confidence: Some(w.confidence),
                    scene_boundary: Some(scene.clone()),
                    is_at_scene_cut: false,
                    scene_index: scene_idx,
                    scene_position: classify_position(scene, w.start),
                    reason: "low_confidence".to_string(),
                    priority,
                });
                seen_keys.insert(key);
            }
        }
    }

    // 3. Sort by priority ascending, then timestamp ascending
    moments.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.timestamp.partial_cmp(&b.timestamp).unwrap_or(std::cmp::Ordering::Equal))
    });

    // 4. Deduplicate by (timestamp, word) key — already done via seen_keys above
    moments
}

/// Format scene boundaries as LLM-readable context.
pub fn format_scene_changes_for_prompt(scenes: &[SceneBoundary]) -> String {
    scenes
        .iter()
        .enumerate()
        .map(|(i, s)| {
            format!(
                "Scene {}: {} - {} ({:.1}s)",
                i + 1,
                format_timestamp(s.start_sec),
                format_timestamp(s.end_sec),
                s.duration_sec
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format fusion data as LLM-readable context.
/// Only includes moments where is_at_scene_cut or confidence < 70.
pub fn format_fusion_data_for_prompt(moments: &[FusedMoment]) -> String {
    let filtered: Vec<&FusedMoment> = moments
        .iter()
        .filter(|m| m.is_at_scene_cut || m.asr_confidence.map_or(false, |c| c < 70))
        .collect();

    if filtered.is_empty() {
        return String::new();
    }

    filtered
        .iter()
        .map(|m| {
            let conf_str = m
                .asr_confidence
                .map(|c| format!("confidence: {}%", c))
                .unwrap_or_else(|| "confidence: N/A".to_string());
            let scene_str = m
                .scene_boundary
                .as_ref()
                .map(|s| {
                    format!(
                        "scene: scene {} ({:.1}s)",
                        m.scene_index + 1,
                        s.duration_sec
                    )
                })
                .unwrap_or_else(|| "scene: unknown".to_string());
            format!(
                "[{}] \"{}\" — {}, {}, position: {}",
                m.timestamp_fmt, m.word, conf_str, scene_str, m.scene_position
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_scene(start: f64, end: f64) -> SceneBoundary {
        SceneBoundary {
            start_sec: start,
            end_sec: end,
            duration_sec: end - start,
            frame_start: (start * 24.0) as u64,
            frame_end: (end * 24.0) as u64,
        }
    }

    fn make_word(word: &str, start: f64, confidence: i32) -> WordTiming {
        WordTiming {
            word: word.to_string(),
            start,
            confidence,
        }
    }

    fn make_segment(start: f64, text: &str, words: Option<Vec<WordTiming>>) -> TranscriptSegment {
        TranscriptSegment {
            start,
            end: start + 5.0,
            text: text.to_string(),
            words,
        }
    }

    #[test]
    fn test_classify_position_at_cut() {
        let scene = make_scene(10.0, 30.0);
        assert_eq!(classify_position(&scene, 10.5), ScenePosition::AtCut);
        assert_eq!(classify_position(&scene, 9.5), ScenePosition::AtCut);
    }

    #[test]
    fn test_classify_position_early() {
        let scene = make_scene(10.0, 30.0);
        assert_eq!(classify_position(&scene, 12.0), ScenePosition::EarlyScene);
    }

    #[test]
    fn test_classify_position_late() {
        let scene = make_scene(10.0, 30.0);
        assert_eq!(classify_position(&scene, 28.0), ScenePosition::LateScene);
    }

    #[test]
    fn test_classify_position_mid() {
        let scene = make_scene(10.0, 30.0);
        assert_eq!(classify_position(&scene, 20.0), ScenePosition::MidScene);
    }

    #[test]
    fn test_find_words_near_boundary() {
        let scene = make_scene(10.0, 30.0);
        let segments = vec![
            make_segment(
                8.0,
                "hello",
                Some(vec![make_word("hello", 8.5, 90), make_word("world", 10.3, 85)]),
            ),
            make_segment(
                15.0,
                "far away",
                Some(vec![make_word("far", 15.0, 90)]),
            ),
        ];
        let near = find_words_near_boundary(&segments, &scene, 1.0);
        assert_eq!(near.len(), 1);
        assert_eq!(near[0].word, "world");
    }

    #[test]
    fn test_find_containing_scene() {
        let scenes = vec![make_scene(0.0, 10.0), make_scene(10.0, 20.0)];
        let (idx, scene) = find_containing_scene(&scenes, 5.0).unwrap();
        assert_eq!(idx, 0);
        assert_eq!(scene.start_sec, 0.0);
        let (idx, _) = find_containing_scene(&scenes, 15.0).unwrap();
        assert_eq!(idx, 1);
        assert!(find_containing_scene(&scenes, 25.0).is_none());
    }

    #[test]
    fn test_fuse_scenes_and_transcript_basic() {
        let scenes = vec![make_scene(0.0, 10.0), make_scene(10.0, 20.0)];
        let segments = vec![make_segment(
            9.5,
            "hello world",
            Some(vec![
                make_word("hello", 9.5, 80),
                make_word("world", 10.2, 60),
            ]),
        )];
        let fused = fuse_scenes_and_transcript(&scenes, &segments, 20.0);
        // Both words should appear: "hello" near scene boundary, "world" low confidence
        assert!(fused.len() >= 2);
    }

    #[test]
    fn test_fuse_deduplication() {
        let scenes = vec![make_scene(0.0, 10.0)];
        let segments = vec![make_segment(
            9.5,
            "test",
            Some(vec![make_word("test", 9.8, 60)]),
        )];
        let fused = fuse_scenes_and_transcript(&scenes, &segments, 10.0);
        // "test" is both near boundary AND low confidence — should appear once
        let test_count = fused.iter().filter(|m| m.word == "test").count();
        assert_eq!(test_count, 1);
        // Should have the scene_boundary_low_confidence reason (priority 1)
        assert_eq!(fused[0].reason, "scene_boundary_low_confidence");
    }

    #[test]
    fn test_format_scene_changes() {
        let scenes = vec![make_scene(0.0, 10.0), make_scene(10.0, 25.0)];
        let output = format_scene_changes_for_prompt(&scenes);
        assert!(output.contains("Scene 1:"));
        assert!(output.contains("Scene 2:"));
        assert!(output.contains("10.0s"));
        assert!(output.contains("15.0s"));
    }

    #[test]
    fn test_format_fusion_data_filters() {
        let scenes = vec![make_scene(0.0, 10.0)];
        let segments = vec![make_segment(
            0.0,
            "test",
            Some(vec![
                make_word("high", 0.5, 95),
                make_word("low", 1.0, 60),
            ]),
        )];
        let fused = fuse_scenes_and_transcript(&scenes, &segments, 10.0);
        let output = format_fusion_data_for_prompt(&fused);
        // "high" is at cut with high confidence — should appear
        assert!(output.contains("high"));
        // "low" is at cut with low confidence — should appear
        assert!(output.contains("low"));
    }

    #[test]
    fn test_format_fusion_data_empty() {
        let output = format_fusion_data_for_prompt(&[]);
        assert!(output.is_empty());
    }
}
