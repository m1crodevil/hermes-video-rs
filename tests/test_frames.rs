use watch2::frames::FrameMeta;

#[test]
fn test_frame_meta_fields() {
    let meta = FrameMeta {
        engine: "scene".into(),
        candidate_count: 10,
        selected_count: 5,
        deduped_count: 3,
        fallback: false,
        dropped_out_of_window: 1,
    };
    assert_eq!(meta.engine, "scene");
    assert_eq!(meta.candidate_count, 10);
    assert_eq!(meta.selected_count, 5);
    assert_eq!(meta.deduped_count, 3);
    assert!(!meta.fallback);
    assert_eq!(meta.dropped_out_of_window, 1);
}

#[test]
fn test_frame_meta_keyframe_engine() {
    let meta = FrameMeta {
        engine: "keyframe".into(),
        candidate_count: 20,
        selected_count: 8,
        deduped_count: 0,
        fallback: false,
        dropped_out_of_window: 0,
    };
    assert_eq!(meta.engine, "keyframe");
    assert_eq!(meta.selected_count, 8);
}

#[test]
fn test_frame_meta_fallback_uniform() {
    let meta = FrameMeta {
        engine: "uniform".into(),
        candidate_count: 3,
        selected_count: 3,
        deduped_count: 1,
        fallback: true,
        dropped_out_of_window: 2,
    };
    assert!(meta.fallback);
    assert_eq!(meta.dropped_out_of_window, 2);
}

#[test]
fn test_frame_meta_timestamps_engine() {
    let meta = FrameMeta {
        engine: "timestamps".into(),
        candidate_count: 5,
        selected_count: 3,
        deduped_count: 0,
        fallback: false,
        dropped_out_of_window: 2,
    };
    assert_eq!(meta.engine, "timestamps");
    assert_eq!(meta.dropped_out_of_window, 2);
}
