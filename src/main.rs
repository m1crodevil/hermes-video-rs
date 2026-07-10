mod cli;
mod config;
mod dedup;
mod download;
mod error;
mod frames;
mod output;
mod scene;
mod setup;
mod timestamp;
mod transcript;
mod whisper;

use clap::Parser;
use cli::Cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    eprintln!("[watch-rs] v{}", env!("CARGO_PKG_VERSION"));
    eprintln!("[watch-rs] source: {}", cli.source);
    eprintln!("[watch-rs] working dir: (will be set in pipeline)");
    // Pipeline will be wired in Task 12
    Ok(())
}
