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
}

impl WatchReport {
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {}\n\n", self.title));
        out.push_str(&format!("**Source:** {} | **Detail:** {} | **Duration:** {}\n\n",
            self.source, self.detail, format_time(self.duration)));
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
            out.push_str(&format!("## Frames ({} total, {} dropped)\n\n",
                self.frames.len(), self.frames_dropped));
            for f in &self.frames {
                out.push_str(&format!("- `{}` (t={}, {})\n",
                    f.path, format_time(f.timestamp), f.reason));
            }
            out.push('\n');
        }
        if !self.transcript.is_empty() {
            out.push_str(&format!("## Transcript ({})\n\n", self.transcript_source));
            for seg in &self.transcript {
                out.push_str(&format!("[{} -> {}] {}\n",
                    format_time(seg.start), format_time(seg.end), seg.text));
            }
            out.push('\n');
        }
        if self.frames.is_empty() && self.transcript.is_empty() {
            out.push_str("*No frames or transcript available.*\n");
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
            title: "Test".into(), source: "test.mp4".into(), detail: "balanced".into(),
            uploader: Some("TestChannel".into()),
            language: Some("en".into()),
            engine: Some("scene-or-uniform".into()),
            frames: vec![], frames_dropped: 0, transcript: vec![],
            transcript_source: "none".into(), duration: 60.0, working_dir: "/tmp/test".into(),
            warnings: vec![],
        };
        let md = report.to_markdown();
        assert!(md.contains("# Test"));
        assert!(md.contains("No frames or transcript available"));
        assert!(md.contains("**Uploader:** TestChannel"));
        assert!(md.contains("**Language:** en"));
        assert!(md.contains("**Engine:** scene-or-uniform"));
    }
}
