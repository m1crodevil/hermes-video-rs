use crate::timestamp::format_time;
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct FrameInfo {
    pub path: String,
    pub timestamp: f64,
    pub reason: String,
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

#[derive(Serialize)]
pub struct AnalysisCapabilities {
    pub transcript: bool,
    pub scene_detection: bool,
    pub frame_extraction: bool,
    pub visual_verification: bool,
}

#[derive(Serialize)]
pub struct WatchReport {
    pub title: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uploader: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub frames: Vec<FrameInfo>,
    pub transcript: Vec<TranscriptSegment>,
    pub transcript_source: String,
    pub video_access: String,
    pub analysis_capabilities: AnalysisCapabilities,
    pub duration: f64,
    pub working_dir: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_boundaries: Option<Vec<crate::scene_detect::SceneBoundary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_count: Option<usize>,
}

impl WatchReport {
    pub fn to_markdown(&self) -> String {
        let mut out = format!(
            "# {}\n\n**Source:** {} | **Duration:** {}\n\n",
            self.title,
            self.source,
            format_time(self.duration)
        );
        if let Some(uploader) = &self.uploader {
            out.push_str(&format!("**Uploader:** {uploader}\n"));
        }
        if let Some(language) = &self.language {
            out.push_str(&format!("**Language:** {language}\n"));
        }
        if self.uploader.is_some() || self.language.is_some() {
            out.push('\n');
        }
        if !self.frames.is_empty() {
            out.push_str(&format!("## Frames ({})\n\n", self.frames.len()));
            for frame in &self.frames {
                out.push_str(&format!(
                    "- `{}` (t={}, {})\n",
                    frame.path,
                    format_time(frame.timestamp),
                    frame.reason
                ));
            }
            out.push('\n');
        }
        if !self.transcript.is_empty() {
            out.push_str(&format!("## Transcript ({})\n\n", self.transcript_source));
            for segment in &self.transcript {
                out.push_str(&format!(
                    "[{} -> {}] {}\n",
                    format_time(segment.start),
                    format_time(segment.end),
                    segment.text
                ));
            }
            out.push('\n');
        }
        if self.frames.is_empty() && self.transcript.is_empty() {
            out.push_str("*No frames or transcript available.*\n");
        }
        if !self.warnings.is_empty() {
            out.push_str("## Warnings\n\n");
            for warning in &self.warnings {
                out.push_str(&format!("- ⚠️ {warning}\n"));
            }
        }
        out
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("WatchReport is serializable")
    }

    pub fn write_json(&self, work_dir: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
        let path = work_dir.join("report.json");
        std::fs::write(&path, self.to_json())?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> WatchReport {
        WatchReport {
            title: "Test".into(),
            source: "test.mp4".into(),
            uploader: Some("Channel".into()),
            language: Some("en".into()),
            frames: vec![],
            transcript: vec![TranscriptSegment {
                start: 0.0,
                end: 1.0,
                text: "hello".into(),
                words: None,
            }],
            transcript_source: "captions".into(),
            video_access: "local".into(),
            analysis_capabilities: AnalysisCapabilities {
                transcript: true,
                scene_detection: false,
                frame_extraction: false,
                visual_verification: false,
            },
            duration: 1.0,
            working_dir: "/tmp/test".into(),
            warnings: vec![],
            scene_boundaries: None,
            scene_count: None,
        }
    }

    #[test]
    fn report_serializes_runtime_fields() {
        let json: serde_json::Value = serde_json::from_str(&report().to_json()).unwrap();
        assert_eq!(json["title"], "Test");
        assert_eq!(json["transcript"][0]["text"], "hello");
        assert_eq!(json["analysis_capabilities"]["visual_verification"], false);
    }

    #[test]
    fn markdown_includes_transcript() {
        assert!(report().to_markdown().contains("hello"));
    }

    #[test]
    fn write_json_persists_report() {
        let dir = tempfile::tempdir().unwrap();
        let path = report().write_json(dir.path()).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(json["title"], "Test");
    }
}
