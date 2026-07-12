use watch_rs::output::{WatchReport, FrameInfo, TranscriptSegment};

fn make_test_report() -> WatchReport {
    WatchReport {
        title: "Test Video".into(),
        source: "test.mp4".into(),
        detail: "balanced".into(),
        frames: vec![],
        frames_dropped: 0,
        transcript: vec![],
        transcript_source: "none".into(),
        duration: 60.0,
        working_dir: "/tmp/test".into(),
        warnings: vec!["Test warning".into()],
    }
}

fn make_full_report() -> WatchReport {
    WatchReport {
        title: "Full Video".into(),
        source: "https://youtu.be/abc".into(),
        detail: "token-burner".into(),
        frames: vec![
            FrameInfo {
                path: "/tmp/frame_0001.jpg".into(),
                timestamp: 0.0,
                reason: "keyframe".into(),
            },
            FrameInfo {
                path: "/tmp/frame_0002.jpg".into(),
                timestamp: 5.5,
                reason: "scene".into(),
            },
        ],
        frames_dropped: 2,
        transcript: vec![
            TranscriptSegment {
                start: 0.0,
                end: 3.0,
                text: "Hello world".into(),
            },
            TranscriptSegment {
                start: 3.0,
                end: 6.0,
                text: "Second segment".into(),
            },
        ],
        transcript_source: "groq".into(),
        duration: 120.0,
        working_dir: "/tmp/full".into(),
        warnings: vec!["Warning one".into(), "Warning two".into()],
    }
}

// --- Markdown tests ---

#[test]
fn test_markdown_contains_title() {
    let report = make_test_report();
    let md = report.to_markdown();
    assert!(md.contains("# Test Video"));
}

#[test]
fn test_markdown_contains_warnings() {
    let report = make_test_report();
    let md = report.to_markdown();
    assert!(md.contains("Test warning"));
}

#[test]
fn test_markdown_empty_frames_transcript() {
    let report = make_test_report();
    let md = report.to_markdown();
    assert!(md.contains("No frames or transcript available"));
}

#[test]
fn test_markdown_source_and_detail() {
    let report = make_test_report();
    let md = report.to_markdown();
    assert!(md.contains("test.mp4"));
    assert!(md.contains("balanced"));
}

#[test]
fn test_markdown_full_report_frames() {
    let report = make_full_report();
    let md = report.to_markdown();
    assert!(md.contains("# Full Video"));
    assert!(md.contains("## Frames (2 total, 2 dropped)"));
    assert!(md.contains("frame_0001.jpg"));
    assert!(md.contains("frame_0002.jpg"));
    assert!(md.contains("keyframe"));
    assert!(md.contains("scene"));
}

#[test]
fn test_markdown_full_report_transcript() {
    let report = make_full_report();
    let md = report.to_markdown();
    assert!(md.contains("## Transcript (groq)"));
    assert!(md.contains("Hello world"));
    assert!(md.contains("Second segment"));
}

#[test]
fn test_markdown_full_report_warnings() {
    let report = make_full_report();
    let md = report.to_markdown();
    assert!(md.contains("Warning one"));
    assert!(md.contains("Warning two"));
}

// --- JSON tests ---

#[test]
fn test_json_output_title() {
    let report = make_test_report();
    let json = report.to_json();
    assert!(json.contains("Test Video"));
}

#[test]
fn test_json_output_warnings() {
    let report = make_test_report();
    let json = report.to_json();
    assert!(json.contains("Test warning"));
}

#[test]
fn test_json_output_valid_json() {
    let report = make_test_report();
    let json = report.to_json();
    // Should be parseable as JSON
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["title"], "Test Video");
}

#[test]
fn test_json_output_full_report() {
    let report = make_full_report();
    let json = report.to_json();
    assert!(json.contains("Full Video"));
    assert!(json.contains("keyframe"));
    assert!(json.contains("Hello world"));
    assert!(json.contains("Warning one"));

    // Verify valid JSON and structure
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["title"], "Full Video");
    assert_eq!(parsed["source"], "https://youtu.be/abc");
    assert_eq!(parsed["detail"], "token-burner");
    assert_eq!(parsed["transcript_source"], "groq");
    // frames array should have 2 items
    assert_eq!(parsed["frames"].as_array().unwrap().len(), 2);
    // transcript array should have 2 items
    assert_eq!(parsed["transcript"].as_array().unwrap().len(), 2);
    // warnings should be skipped when non-empty, present here
    assert_eq!(parsed["warnings"].as_array().unwrap().len(), 2);
}

#[test]
fn test_json_warnings_skipped_when_empty() {
    let mut report = make_test_report();
    report.warnings = vec![];
    let json = report.to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    // warnings field should be absent due to skip_serializing_if
    assert!(parsed.get("warnings").is_none());
}

#[test]
fn test_json_has_all_metadata() {
    let report = make_test_report();
    let json = report.to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["source"], "test.mp4");
    assert_eq!(parsed["detail"], "balanced");
    assert_eq!(parsed["duration"], 60.0);
    assert_eq!(parsed["working_dir"], "/tmp/test");
    assert_eq!(parsed["transcript_source"], "none");
}
