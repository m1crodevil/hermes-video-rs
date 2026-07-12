use watch2::transcript::{parse_json3, parse_vtt};

#[test]
fn test_parse_json3_basic() {
    let json = r#"{"events":[{"tStartMs":0,"dDurationMs":2000,"segs":[{"utf8":"Hello "},{"utf8":"world"}]}]}"#;
    let segs = parse_json3(json).unwrap();
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].text, "Hello world");
    assert!((segs[0].start - 0.0).abs() < 0.01);
    assert!((segs[0].end - 2.0).abs() < 0.01);
}

#[test]
fn test_parse_json3_empty() {
    let json = r#"{"events":[]}"#;
    let segs = parse_json3(json).unwrap();
    assert_eq!(segs.len(), 0);
}

#[test]
fn test_parse_json3_dedup() {
    let json = r#"{"events":[{"tStartMs":0,"dDurationMs":1000,"segs":[{"utf8":"hello"}]},{"tStartMs":1000,"dDurationMs":1000,"segs":[{"utf8":"hello"}]}]}"#;
    let segs = parse_json3(json).unwrap();
    assert_eq!(segs.len(), 1);
}

#[test]
fn test_parse_vtt_basic() {
    let vtt = "WEBVTT\n\n00:00:00.000 --> 00:00:02.000\nHello world\n";
    let segs = parse_vtt(vtt).unwrap();
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].text, "Hello world");
}

#[test]
fn test_parse_vtt_multi_line() {
    let vtt = "WEBVTT\n\n00:00:00.000 --> 00:00:02.000\nLine one\nLine two\n";
    let segs = parse_vtt(vtt).unwrap();
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].text, "Line one Line two");
}
