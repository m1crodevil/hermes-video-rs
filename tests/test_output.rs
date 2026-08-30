use watch2::output::{FrameInfo, TranscriptSegment, WatchReport};

fn report() -> WatchReport {
    WatchReport {
        title: "Test Video".into(),
        source: "https://example.com/video".into(),
        uploader: Some("Creator".into()),
        language: Some("en".into()),
        frames: vec![FrameInfo {
            path: "frame.jpg".into(),
            timestamp: 10.0,
            reason: "transcript-cue".into(),
        }],
        transcript: vec![TranscriptSegment {
            start: 0.0,
            end: 1.0,
            text: "hello".into(),
            words: None,
        }],
        transcript_source: "captions".into(),
        duration: 60.0,
        working_dir: "/tmp/watch2".into(),
        warnings: vec![],
        scene_boundaries: None,
        scene_count: None,
    }
}

#[test]
fn json_contains_extraction_evidence() {
    let json: serde_json::Value = serde_json::from_str(&report().to_json()).unwrap();
    assert_eq!(json["title"], "Test Video");
    assert_eq!(json["frames"][0]["timestamp"], 10.0);
    assert_eq!(json["transcript"][0]["text"], "hello");
    assert!(json.get("scene_boundaries").is_none());
}

#[test]
fn markdown_contains_frames_and_transcript() {
    let markdown = report().to_markdown();
    assert!(markdown.contains("frame.jpg"));
    assert!(markdown.contains("hello"));
}
