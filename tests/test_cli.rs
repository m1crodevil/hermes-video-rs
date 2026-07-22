use clap::Parser;
use watch2::cli::{Cli, OutputFormat};

#[test]
fn test_parse_basic_url() {
    let cli = Cli::try_parse_from(["watch", "https://youtu.be/abc"]).unwrap();
    assert_eq!(cli.source, "https://youtu.be/abc");
    assert_eq!(cli.resolution, 512);
    assert!(!cli.no_whisper);
    assert!(!cli.no_dedup);
}

#[test]
fn test_parse_with_flags() {
    let cli = Cli::try_parse_from([
        "watch", "test.mp4",
        "--no-whisper",
    ]).unwrap();
    assert_eq!(cli.source, "test.mp4");
    assert!(cli.no_whisper);
}

#[test]
fn test_output_format_json() {
    let cli = Cli::try_parse_from(["watch", "test.mp4", "--output", "json"]).unwrap();
    assert_eq!(cli.output, OutputFormat::Json);
}

#[test]
fn test_output_format_both() {
    let cli = Cli::try_parse_from(["watch", "test.mp4", "--output", "both"]).unwrap();
    assert_eq!(cli.output, OutputFormat::Both);
}

#[test]
fn test_output_format_markdown_default() {
    let cli = Cli::try_parse_from(["watch", "test.mp4"]).unwrap();
    assert_eq!(cli.output, OutputFormat::Markdown);
}

#[test]
fn test_keep_video_flag() {
    let cli = Cli::try_parse_from(["watch", "test.mp4", "--keep-video"]).unwrap();
    assert!(cli.keep_video);
}

#[test]
fn test_keep_video_default_false() {
    let cli = Cli::try_parse_from(["watch", "test.mp4"]).unwrap();
    assert!(!cli.keep_video);
}

#[test]
fn test_no_whisper_flag() {
    let cli = Cli::try_parse_from(["watch", "test.mp4", "--no-whisper"]).unwrap();
    assert!(cli.no_whisper);
}

#[test]
fn test_no_whisper_default_false() {
    let cli = Cli::try_parse_from(["watch", "test.mp4"]).unwrap();
    assert!(!cli.no_whisper);
}

#[test]
fn test_no_dedup_flag() {
    let cli = Cli::try_parse_from(["watch", "test.mp4", "--no-dedup"]).unwrap();
    assert!(cli.no_dedup);
}

#[test]
fn test_no_dedup_default_false() {
    let cli = Cli::try_parse_from(["watch", "test.mp4"]).unwrap();
    assert!(!cli.no_dedup);
}

#[test]
fn test_resolution_default() {
    let cli = Cli::try_parse_from(["watch", "test.mp4"]).unwrap();
    assert_eq!(cli.resolution, 512);
}

#[test]
fn test_resolution_custom() {
    let cli = Cli::try_parse_from(["watch", "test.mp4", "--resolution", "1024"]).unwrap();
    assert_eq!(cli.resolution, 1024);
}

#[test]
fn test_out_dir_flag() {
    let cli = Cli::try_parse_from(["watch", "test.mp4", "--out-dir", "/tmp/test-tm"]).unwrap();
    assert_eq!(cli.out_dir, Some("/tmp/test-tm".to_string()));
}

#[test]
fn test_out_dir_default_none() {
    let cli = Cli::try_parse_from(["watch", "test.mp4"]).unwrap();
    assert!(cli.out_dir.is_none());
}

#[test]
fn test_no_cache_flag() {
    let cli = Cli::try_parse_from(["watch", "test.mp4", "--no-cache"]).unwrap();
    assert!(cli.no_cache);
}

#[test]
fn test_cache_dir_flag() {
    let cli = Cli::try_parse_from(["watch", "test.mp4", "--cache-dir", "/tmp/cache"]).unwrap();
    assert_eq!(cli.cache_dir, Some("/tmp/cache".to_string()));
}

#[test]
fn test_cookies_flag() {
    let cli = Cli::try_parse_from(["watch", "test.mp4", "--cookies"]).unwrap();
    assert!(cli.cookies);
}

#[test]
fn test_multiple_flags_combined() {
    let cli = Cli::try_parse_from([
        "watch", "https://youtu.be/abc",
        "--no-whisper",
        "--no-dedup",
        "--keep-video",
        "--output", "json",
        "--resolution", "768",
        "--out-dir", "/tmp/test-combo",
    ]).unwrap();
    assert_eq!(cli.source, "https://youtu.be/abc");
    assert!(cli.no_whisper);
    assert!(cli.no_dedup);
    assert!(cli.keep_video);
    assert_eq!(cli.output, OutputFormat::Json);
    assert_eq!(cli.resolution, 768);
    assert_eq!(cli.out_dir, Some("/tmp/test-combo".to_string()));
}
