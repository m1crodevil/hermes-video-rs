use clap::Parser;
use watch_rs::cli::Cli;

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
