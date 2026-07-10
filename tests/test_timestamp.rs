use watch_rs::timestamp::{parse_time, format_time};

#[test]
fn test_parse_seconds() {
    assert!((parse_time(Some("45.5")).unwrap() - 45.5).abs() < 0.01);
}

#[test]
fn test_parse_mm_ss() {
    assert!((parse_time(Some("2:30")).unwrap() - 150.0).abs() < 0.01);
}

#[test]
fn test_parse_hh_mm_ss() {
    assert!((parse_time(Some("1:30:00")).unwrap() - 5400.0).abs() < 0.01);
}

#[test]
fn test_parse_none() {
    assert!(parse_time(None).is_none());
}

#[test]
fn test_parse_empty() {
    assert!(parse_time(Some("")).is_none());
}

#[test]
fn test_format_time() {
    assert_eq!(format_time(90.0), "01:30");
}

#[test]
fn test_format_hours() {
    assert_eq!(format_time(3661.0), "1:01:01");
}

#[test]
fn test_format_zero() {
    assert_eq!(format_time(0.0), "00:00");
}
