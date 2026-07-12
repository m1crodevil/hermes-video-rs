use watch_rs::download::VideoInfo;

#[test]
fn test_video_info_default() {
    let info = VideoInfo::default();
    assert_eq!(info.title, "Unknown");
    assert!(info.uploader.is_none());
    assert!(info.duration.is_none());
    assert!(info.language.is_none());
    assert!(info.description.is_none());
}

#[test]
fn test_video_info_with_values() {
    let info = VideoInfo {
        title: "My Video".to_string(),
        uploader: Some("Channel".to_string()),
        duration: Some(120.5),
        language: Some("en".to_string()),
        description: Some("A description".to_string()),
    };
    assert_eq!(info.title, "My Video");
    assert_eq!(info.uploader.as_deref(), Some("Channel"));
    assert!((info.duration.unwrap() - 120.5).abs() < 0.01);
    assert_eq!(info.language.as_deref(), Some("en"));
    assert_eq!(info.description.as_deref(), Some("A description"));
}

#[test]
fn test_video_info_serialize() {
    let info = VideoInfo::default();
    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("\"Unknown\""));
}

#[test]
fn test_video_info_deserialize() {
    let json = r#"{"title":"Test","uploader":"Bob","duration":90.0,"language":"id","description":"Hello"}"#;
    let info: VideoInfo = serde_json::from_str(json).unwrap();
    assert_eq!(info.title, "Test");
    assert_eq!(info.uploader.as_deref(), Some("Bob"));
    assert!((info.duration.unwrap() - 90.0).abs() < 0.01);
    assert_eq!(info.language.as_deref(), Some("id"));
}

#[test]
fn test_video_info_clone() {
    let info = VideoInfo::default();
    let cloned = info.clone();
    assert_eq!(cloned.title, "Unknown");
}
