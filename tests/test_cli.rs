use clap::Parser;
use watch2::cli::{Cli, DetailMode, OutputFormat};

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
        "--detail", "efficient",
        "--max-frames", "50",
        "--no-whisper",
    ]).unwrap();
    assert_eq!(cli.source, "test.mp4");
    assert!(cli.max_frames == Some(50));
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
fn test_timestamps_flag() {
    let cli = Cli::try_parse_from(["watch", "test.mp4", "--timestamps", "5,10,15"]).unwrap();
    assert_eq!(cli.timestamps, Some("5,10,15".to_string()));
}

#[test]
fn test_timestamps_default_none() {
    let cli = Cli::try_parse_from(["watch", "test.mp4"]).unwrap();
    assert!(cli.timestamps.is_none());
}

#[test]
fn test_parse_transcript_moments() {
    let cli = Cli::try_parse_from([
        "watch", "https://youtu.be/abc",
        "--detail", "transcript-moments",
        "--min-moments", "50",
        "--out-dir", "/tmp/test-tm",
    ]).unwrap();
    assert_eq!(cli.detail, Some(DetailMode::TranscriptMoments));
    assert_eq!(cli.min_moments, Some(50));
    assert_eq!(cli.out_dir, Some("/tmp/test-tm".to_string()));
}

#[test]
fn test_auto_moments_flag() {
    let cli = Cli::try_parse_from([
        "watch", "test.mp4",
        "--auto-moments",
        "--max-moments", "30",
    ]).unwrap();
    assert!(cli.auto_moments);
    assert_eq!(cli.max_moments, 30);
}
