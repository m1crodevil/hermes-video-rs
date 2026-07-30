use crate::timestamp::format_time;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Clone)]
pub struct FrameInfo {
    pub path: String,
    pub timestamp: f64,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_score: Option<f64>,
}

#[derive(Serialize, Clone)]
pub struct WordTiming {
    pub word: String,
    pub start: f64,
    pub confidence: i32,
}

#[derive(Serialize, Clone)]
pub struct TranscriptSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub words: Option<Vec<WordTiming>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KeyMomentStats {
    pub total: usize,
    pub by_reason: HashMap<String, usize>,
    pub by_priority: HashMap<u32, usize>,
}

#[derive(Serialize)]
pub struct WatchReport {
    pub title: String,
    pub source: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uploader: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    pub frames: Vec<FrameInfo>,
    pub frames_dropped: u32,
    pub transcript: Vec<TranscriptSegment>,
    pub transcript_source: String,
    pub duration: f64,
    pub working_dir: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_moments: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_moment_stats: Option<KeyMomentStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_boundaries: Option<Vec<crate::scene_detect::SceneBoundary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_scores_path: Option<String>,
}

impl WatchReport {
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {}\n\n", self.title));
        out.push_str(&format!(
            "**Source:** {} | **Detail:** {} | **Duration:** {}\n\n",
            self.source,
            self.detail,
            format_time(self.duration)
        ));
        if let Some(ref u) = self.uploader {
            out.push_str(&format!("**Uploader:** {}\n", u));
        }
        if let Some(ref l) = self.language {
            out.push_str(&format!("**Language:** {}\n", l));
        }
        if let Some(ref e) = self.engine {
            out.push_str(&format!("**Engine:** {}\n", e));
        }
        if self.uploader.is_some() || self.language.is_some() || self.engine.is_some() {
            out.push('\n');
        }
        if !self.frames.is_empty() {
            out.push_str(&format!(
                "## Frames ({} total, {} dropped)\n\n",
                self.frames.len(),
                self.frames_dropped
            ));
            for f in &self.frames {
                out.push_str(&format!(
                    "- `{}` (t={}, {})\n",
                    f.path,
                    format_time(f.timestamp),
                    f.reason
                ));
            }
            out.push('\n');
        }
        if !self.transcript.is_empty() {
            out.push_str(&format!("## Transcript ({})\n\n", self.transcript_source));
            for seg in &self.transcript {
                out.push_str(&format!(
                    "[{} -> {}] {}\n",
                    format_time(seg.start),
                    format_time(seg.end),
                    seg.text
                ));
            }
            out.push('\n');
        }
        if let Some(ref moments) = self.key_moments {
            if !moments.is_empty() {
                out.push_str(&format!("## Key Moments ({})\n\n", moments.len()));
                for m in moments {
                    let ts = m.get("timestamp").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let word = m.get("word").and_then(|v| v.as_str()).unwrap_or("?");
                    let reason = m.get("reason").and_then(|v| v.as_str()).unwrap_or("?");
                    let priority = m.get("priority").and_then(|v| v.as_u64()).unwrap_or(3);
                    out.push_str(&format!(
                        "- `[{}]` P{} `{}` ({})\n",
                        format_time(ts),
                        priority,
                        word,
                        reason
                    ));
                }
                out.push('\n');
            }
        }
        if self.frames.is_empty() && self.transcript.is_empty() {
            out.push_str("*No frames or transcript available.*\n");
        }
        if let Some(ref p) = self.scene_scores_path {
            out.push_str(&format!("**Scene Scores:** `{}`\n", p));
        }
        if !self.warnings.is_empty() {
            out.push_str("## Warnings\n\n");
            for w in &self.warnings {
                out.push_str(&format!("- ⚠️ {}\n", w));
            }
            out.push('\n');
        }
        out
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_report() {
        let report = WatchReport {
            title: "Test".into(),
            source: "test.mp4".into(),
            detail: "balanced".into(),
            uploader: Some("TestChannel".into()),
            language: Some("en".into()),
            engine: Some("scene-or-uniform".into()),
            frames: vec![],
            frames_dropped: 0,
            transcript: vec![],
            transcript_source: "none".into(),
            duration: 60.0,
            working_dir: "/tmp/test".into(),
            warnings: vec![],
            key_moments: None,
            key_moment_stats: None,
            scene_boundaries: None,
            scene_count: None,
            scene_scores_path: None,
        };
        let md = report.to_markdown();
        assert!(md.contains("# Test"));
        assert!(md.contains("No frames or transcript available"));
        assert!(md.contains("**Uploader:** TestChannel"));
        assert!(md.contains("**Language:** en"));
        assert!(md.contains("**Engine:** scene-or-uniform"));
    }

    #[test]
    fn test_watch_report_json_roundtrip_title() {
        // WatchReport doesn't derive Deserialize, so we roundtrip via serde_json::Value
        let report = WatchReport {
            title: "Roundtrip Test Title".into(),
            source: "https://example.com/video".into(),
            detail: "balanced".into(),
            uploader: Some("Creator".into()),
            language: Some("en".into()),
            engine: Some("uniform".into()),
            frames: vec![],
            frames_dropped: 0,
            transcript: vec![],
            transcript_source: "none".into(),
            duration: 42.5,
            working_dir: "/tmp".into(),
            warnings: vec![],
            key_moments: None,
            key_moment_stats: None,
            scene_boundaries: None,
            scene_count: None,
            scene_scores_path: None,
        };

        let json_str = report.to_json();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("valid JSON");

        assert_eq!(parsed["title"].as_str().unwrap(), "Roundtrip Test Title");
        assert_eq!(
            parsed["source"].as_str().unwrap(),
            "https://example.com/video"
        );
        assert_eq!(parsed["duration"].as_f64().unwrap(), 42.5);
    }

    #[test]
    fn test_empty_report_valid_json() {
        let report = WatchReport {
            title: "Empty".into(),
            source: "none".into(),
            detail: "none".into(),
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

        let json_str = report.to_json();
        // Should parse without error
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).expect("empty report should produce valid JSON");
        assert_eq!(parsed["title"].as_str().unwrap(), "Empty");
        assert_eq!(parsed["duration"].as_f64().unwrap(), 0.0);
    }

    #[test]
    fn test_none_fields_omitted_from_json() {
        let report = WatchReport {
            title: "Omit Test".into(),
            source: "s".into(),
            detail: "d".into(),
            uploader: None,
            language: None,
            engine: None,
            frames: vec![],
            frames_dropped: 0,
            transcript: vec![],
            transcript_source: "none".into(),
            duration: 10.0,
            working_dir: "/tmp".into(),
            warnings: vec![],
            key_moments: None,
            key_moment_stats: None,
            scene_boundaries: None,
            scene_count: None,
            scene_scores_path: None,
        };

        let parsed: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();

        // Option fields set to None should NOT appear in JSON
        assert!(
            parsed.get("uploader").is_none(),
            "uploader should be omitted when None"
        );
        assert!(
            parsed.get("language").is_none(),
            "language should be omitted when None"
        );
        assert!(
            parsed.get("engine").is_none(),
            "engine should be omitted when None"
        );
        assert!(
            parsed.get("key_moments").is_none(),
            "key_moments should be omitted when None"
        );
        assert!(
            parsed.get("key_moment_stats").is_none(),
            "key_moment_stats should be omitted when None"
        );
        assert!(
            parsed.get("scene_boundaries").is_none(),
            "scene_boundaries should be omitted when None"
        );
        assert!(
            parsed.get("scene_count").is_none(),
            "scene_count should be omitted when None"
        );
        assert!(
            parsed.get("scene_scores_path").is_none(),
            "scene_scores_path should be omitted when None"
        );

        // But required fields should still be present
        assert!(parsed.get("title").is_some());
        assert!(parsed.get("source").is_some());
        assert!(parsed.get("frames").is_some());
    }

    #[test]
    fn test_some_fields_present_in_json() {
        let report = WatchReport {
            title: "Some Fields".into(),
            source: "s".into(),
            detail: "d".into(),
            uploader: Some("Creator".into()),
            language: Some("ja".into()),
            engine: Some("groq".into()),
            frames: vec![],
            frames_dropped: 0,
            transcript: vec![],
            transcript_source: "none".into(),
            duration: 5.0,
            working_dir: "/tmp".into(),
            warnings: vec!["test warning".into()],
            key_moments: Some(vec![]),
            key_moment_stats: None,
            scene_boundaries: None,
            scene_count: Some(3),
            scene_scores_path: None,
        };

        let parsed: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();

        assert_eq!(parsed["uploader"].as_str().unwrap(), "Creator");
        assert_eq!(parsed["language"].as_str().unwrap(), "ja");
        assert_eq!(parsed["engine"].as_str().unwrap(), "groq");
        assert_eq!(parsed["scene_count"].as_u64().unwrap(), 3);
        // warnings is a non-empty Vec, should be present
        assert!(parsed["warnings"].is_array());
        assert_eq!(parsed["warnings"].as_array().unwrap().len(), 1);
    }
}
