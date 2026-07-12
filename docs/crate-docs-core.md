# Core Crate API Documentation for hermes-video-rs

> Comprehensive API reference for the core crates used in hermes-video-rs.
> Generated from docs.rs, Context7, and official documentation.

---

## Table of Contents

1. [reqwest — Multipart Upload](#1-reqwest--multipart-upload)
2. [clap — Derive API](#2-clap--derive-api)
3. [serde / serde_json](#3-serde--serde_json)
4. [tempfile](#4-tempfile)
5. [which](#5-which)

---

## 1. reqwest — Multipart Upload

**Crate**: `reqwest 0.12+` (async)  
**Features required**: `multipart`, `stream`  
**Module**: `reqwest::multipart`

### Cargo.toml

```toml
[dependencies]
reqwest = { version = "0.12", features = ["multipart", "stream", "json"] }
```

### Key Types

- `reqwest::multipart::Form` — A multipart/form-data request builder
- `reqwest::multipart::Part` — A single field in a multipart form

---

### Form API

#### `Form::new()` — Create empty form

```rust
pub fn new() -> Form
```

Creates a new empty multipart form builder.

```rust
let form = reqwest::multipart::Form::new();
```

#### `Form::text()` — Add text field

```rust
pub fn text<T, U>(self, name: T, value: U) -> Form
where
    T: Into<Cow<'static, str>>,
    U: Into<Cow<'static, str>>,
```

Add a data field with supplied name and value. Builder pattern (returns self).

```rust
let form = reqwest::multipart::Form::new()
    .text("username", "alice")
    .text("password", "secret");
```

#### `Form::part()` — Add a Part

```rust
pub fn part<T>(self, name: T, part: Part) -> Form
where
    T: Into<Cow<'static, str>>,
```

Adds a pre-built `Part` to the form. Builder pattern.

```rust
let form = reqwest::multipart::Form::new()
    .text("model", "whisper-1")
    .part("file", audio_part);
```

#### `Form::file()` — Add file field (requires `stream` feature)

```rust
pub async fn file<T, U>(self, name: T, path: U) -> Result<Form>
where
    T: Into<Cow<'static, str>>,
    U: AsRef<Path>,
```

Adds a file field. The path is used to guess filename and MIME type.

```rust
let form = reqwest::multipart::Form::new()
    .file("avatar", "/path/to/avatar.png").await?;
```

> **NOTE**: `Form::file()` is `async` because it opens the file.

#### `Form::into_stream()` — Convert to byte stream

```rust
pub fn into_stream(self) -> impl Stream<Item = Result<Bytes, Error>> + Send + Sync
```

Produces a stream of bytes for the entire form body.

---

### Part API

#### `Part::text()` — Create text part

```rust
pub fn text<T>(value: T) -> Part
where
    T: Into<Cow<'static, str>>,
```

Makes a text parameter. Used for inline text content.

```rust
let part = reqwest::multipart::Part::text("whisper-1");
```

#### `Part::bytes()` — Create part from byte data

```rust
pub fn bytes<T>(value: T) -> Part
where
    T: Into<Cow<'static, [u8]>>,
```

Makes a new parameter from arbitrary bytes.

```rust
let file = std::fs::read("/path/to/audio.wav")?;
let part = reqwest::multipart::Part::bytes(file);
```

#### `Part::file()` — Create part from file path (requires `stream` feature)

```rust
pub async fn file<T>(path: T) -> Result<Part>
where
    T: AsRef<Path>,
```

Makes a file parameter. Async because it opens the file.

```rust
let part = reqwest::multipart::Part::file("/path/to/audio.wav").await?;
```

#### `Part::stream()` — Create part from async stream

```rust
pub fn stream<T>(value: T) -> Part
where
    T: Into<Body>,
```

Makes a new parameter from an arbitrary stream. Any type that converts into `reqwest::Body` works.

#### `Part::stream_with_length()` — Stream with known length

```rust
pub fn stream_with_length<T>(value: T, length: u64) -> Part
where
    T: Into<Body>,
```

Like `stream()` but with a known content length. Useful for file streams where you know the size.

#### `Part::file_name()` — Set filename

```rust
pub fn file_name<T>(self, filename: T) -> Part
where
    T: Into<Cow<'static, str>>,
```

Sets the filename, builder style.

```rust
let part = reqwest::multipart::Part::bytes(file_bytes)
    .file_name("recording.wav");
```

#### `Part::mime_str()` — Set MIME type

```rust
pub fn mime_str(self, mime: &str) -> Result<Part>
```

Tries to set the MIME type of this part.

```rust
let part = reqwest::multipart::Part::bytes(audio_data)
    .file_name("audio.wav")
    .mime_str("audio/wav")?;
```

#### `Part::headers()` — Set custom headers

```rust
pub fn headers(self, headers: HeaderMap) -> Part
```

Sets custom headers for the part.

---

### Complete Example: Upload Audio to Whisper API

```rust
use reqwest::multipart;
use std::path::Path;

async fn transcribe_audio(
    api_key: &str,
    audio_path: &Path,
    model: &str,
) -> Result<String, reqwest::Error> {
    let client = reqwest::Client::new();

    // Method 1: Using Part::bytes (read file into memory first)
    let audio_bytes = std::fs::read(audio_path).expect("Failed to read audio file");
    let file_part = multipart::Part::bytes(audio_bytes)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .expect("Invalid MIME type");

    let form = multipart::Form::new()
        .text("model", model.to_string())
        .part("file", file_part);

    let response = client
        .post("https://api.openai.com/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {}", api_key))
        .multipart(form)
        .send()
        .await?;

    let body = response.text().await?;
    Ok(body)
}

// Method 2: Using Form::file (reads file directly)
async fn transcribe_audio_simple(
    api_key: &str,
    audio_path: &str,
) -> Result<String, reqwest::Error> {
    let client = reqwest::Client::new();

    let form = multipart::Form::new()
        .text("model", "whisper-1")
        .text("response_format", "verbose_json")
        .file("file", audio_path).await?;

    let response = client
        .post("https://api.openai.com/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {}", api_key))
        .multipart(form)
        .send()
        .await?;

    let body = response.text().await?;
    Ok(body)
}

// Method 3: Streaming large files without loading into memory
async fn transcribe_large_audio(
    api_key: &str,
    audio_path: &std::path::Path,
) -> Result<String, reqwest::Error> {
    use tokio::fs::File;
    use tokio_util::io::ReaderStream;

    let file = File::open(audio_path).await?;
    let stream = ReaderStream::new(file);

    let file_part = multipart::Part::stream(reqwest::Body::wrap_stream(stream))
        .file_name("audio.wav")
        .mime_str("audio/wav")?;

    let file_len = tokio::fs::metadata(audio_path).await?.len();
    let file_part = multipart::Part::stream_with_length(
        reqwest::Body::wrap_stream(stream),
        file_len,
    )
    .file_name("audio.wav")
    .mime_str("audio/wav")?;

    let form = multipart::Form::new()
        .text("model", "whisper-1")
        .part("file", file_part);

    let response = client
        .post("https://api.openai.com/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {}", api_key))
        .multipart(form)
        .send()
        .await?;

    Ok(response.text().await?)
}
```

### Streaming with `Body::wrap_stream()`

```rust
use reqwest::Body;
use tokio_util::io::ReaderStream;
use tokio::fs::File;

// Wrap an async reader stream into a reqwest Body
let file = File::open("large_audio.wav").await?;
let stream = ReaderStream::new(file);
let body = Body::wrap_stream(stream);

// Then use it in a Part
let part = multipart::Part::stream(body)
    .file_name("large_audio.wav")
    .mime_str("audio/wav")?;
```

> **IMPORTANT**: `Body::wrap_stream()` requires the `stream` feature and takes any
> `impl futures_core::Stream<Item = Result<Bytes, E>>` where `E: Into<Box<dyn Error + Send + Sync>>`.
> `tokio_util::io::ReaderStream` produces exactly this.

### reqwest Feature Flags for Multipart

| Feature | What it enables |
|---------|----------------|
| `multipart` | Enables `reqwest::multipart` module |
| `stream` | Enables `Part::file()`, `Form::file()`, `Part::stream()`, `Body::wrap_stream()` |
| `json` | Enables `.json()` body helper |

Minimum Cargo.toml for Whisper API usage:

```toml
reqwest = { version = "0.12", features = ["multipart", "stream", "json"] }
```

---

## 2. clap — Derive API

**Crate**: `clap 4`  
**Features required**: `derive`

### Cargo.toml

```toml
clap = { version = "4", features = ["derive"] }
```

### Basic Derive Structure

```rust
use clap::{Parser, ValueEnum};

#[derive(Parser)]
#[command(name = "hermes-video", version, about = "Video processing tool")]
struct Cli {
    // fields go here
}
```

---

### ValueEnum: `--output json|markdown|both`

```rust
use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, ValueEnum)]
enum OutputFormat {
    Json,
    Markdown,
    Both,
}

#[derive(Parser)]
#[command(version, about = "Video transcription tool")]
struct Cli {
    /// Output format for transcription results
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,
}

fn main() {
    let cli = Cli::parse();

    match cli.output {
        OutputFormat::Json => println!("JSON output"),
        OutputFormat::Markdown => println!("Markdown output"),
        OutputFormat::Both => println!("Both formats"),
    }
}
```

**CLI behavior:**
- `--output json` → `OutputFormat::Json`
- `--output markdown` → `OutputFormat::Markdown`
- `--output both` → `OutputFormat::Both`
- `--output Json` → `OutputFormat::Json` (case-insensitive by default)
- Omitted → `OutputFormat::Json` (default)

**Custom variant names** (snake_case → different CLI name):

```rust
#[derive(Debug, Clone, ValueEnum)]
enum OutputFormat {
    #[value(name = "json")]
    Json,

    #[value(name = "md")]
    Markdown,  // CLI shows "md" not "markdown"

    #[value(name = "both")]
    Both,
}
```

---

### Bool Flag: `--keep-video`

```rust
use clap::Parser;

#[derive(Parser)]
struct Cli {
    /// Keep the original video file after processing
    #[arg(long)]
    keep_video: bool,
}
```

**CLI behavior:**
- Omitted → `false`
- `--keep-video` → `true`
- `--keep-video=true` → `true`
- `--keep-video=false` → `false`

With short alias:

```rust
#[derive(Parser)]
struct Cli {
    /// Keep the original video file after processing
    #[arg(short = 'k', long)]
    keep_video: bool,
}
```

With `num_args = 0..=1` (allows explicit true/false values):

```rust
#[derive(Parser)]
struct Cli {
    #[arg(long, num_args = 0..=1)]
    keep_video: bool,
}
```

---

### Optional String: `--timestamps`

```rust
use clap::Parser;

#[derive(Parser)]
struct Cli {
    /// Include timestamps in output (format: "srt", "vtt", or "text")
    #[arg(long)]
    timestamps: Option<String>,
}
```

**CLI behavior:**
- Omitted → `None`
- `--timestamps srt` → `Some("srt")`
- `--timestamps=vtt` → `Some("vtt")`

With short alias and value hint:

```rust
#[derive(Parser)]
struct Cli {
    /// Include timestamps in output (srt, vtt, or text)
    #[arg(short = 't', long, value_parser = ["srt", "vtt", "text"])]
    timestamps: Option<String>,
}
```

This restricts values to `srt`, `vtt`, `text` — invalid values cause an error.

---

### Complete CLI Example

```rust
use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, ValueEnum)]
enum OutputFormat {
    Json,
    Markdown,
    Both,
}

#[derive(Parser)]
#[command(
    name = "hermes-video",
    version,
    about = "Process videos: download, transcribe, summarize"
)]
struct Cli {
    /// Video URL or local file path
    input: String,

    /// Output format for transcription results
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,

    /// Keep the original video file after processing
    #[arg(short = 'k', long)]
    keep_video: bool,

    /// Include timestamps in output (srt, vtt, or text)
    #[arg(short = 't', long)]
    timestamps: Option<String>,

    /// Whisper API key (or set OPENAI_API_KEY env var)
    #[arg(long, env = "OPENAI_API_KEY")]
    api_key: Option<String>,
}
```

---

### Useful clap Patterns

**Default value from ValueEnum:**
```rust
#[arg(value_enum, default_value_t = OutputFormat::Json)]
output: OutputFormat,
```

**Positional argument:**
```rust
#[arg()]
input: String,  // required positional
```

**Optional positional (can be omitted):**
```rust
#[arg()]
input: Option<String>,
```

**Multiple values:**
```rust
#[arg(long, num_args = 1..)]
tags: Vec<String>,
```

**Help override:**
```rust
#[arg(long, help = "Custom help message")]
special_flag: bool,
```

---

## 3. serde / serde_json

**Crates**: `serde 1` + `serde_json 1`  
**Features required**: `serde/derive`

### Cargo.toml

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

### Basic Derive

```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
struct UserProfile {
    name: String,
    email: String,
    age: u32,
}

fn main() {
    let user = UserProfile {
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
        age: 30,
    };

    // Serialize to JSON string
    let json = serde_json::to_string(&user).unwrap();
    println!("{}", json);
    // {"name":"Alice","email":"alice@example.com","age":30}

    // Deserialize from JSON string
    let parsed: UserProfile = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.name, "Alice");
}
```

### `serde_json::to_string_pretty()`

```rust
pub fn to_string_pretty<T>(value: &T) -> Result<String>
where
    T: ?Sized + Serialize,
```

Serialize the given data structure as a pretty-printed String of JSON.

```rust
let user = UserProfile {
    name: "Alice".to_string(),
    email: "alice@example.com".to_string(),
    age: 30,
};

let json = serde_json::to_string_pretty(&user).unwrap();
println!("{}", json);
```

Output:
```json
{
  "name": "Alice",
  "email": "alice@example.com",
  "age": 30
}
```

### `#[serde(skip_serializing_if)]`

Skip a field during serialization if the predicate returns true. Useful for
Optional fields that should be omitted from JSON when `None`.

```rust
use serde::Serialize;

#[derive(Serialize)]
struct TranscriptionResult {
    text: String,
    language: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    timestamps: Option<Vec<TimestampSegment>>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    words: Vec<Word>,

    #[serde(skip_serializing_if = "is_zero")]
    duration_secs: f64,
}

fn is_zero(val: &f64) -> bool {
    *val == 0.0
}
```

**Common predicates:**
- `Option::is_none` — skip `None` fields
- `Vec::is_empty` — skip empty vectors
- Custom functions for other conditions

### `#[serde(rename_all = "snake_case")]`

Renames all fields from Rust's `snake_case` to another convention during
serialization/deserialization.

```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
struct WhisperResponse {
    task: String,
    language: String,
    duration: f64,
    text: String,
    segments: Vec<Segment>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
struct Segment {
    start: f64,
    end: f64,
    text: String,
}
```

If the Rust field is already `snake_case`, this is a no-op. But if you have:
```rust
#[serde(rename_all = "camelCase")]
struct Config {
    max_retries: u32,  // serializes as "maxRetries"
}
```

**Available `rename_all` values:**
- `"snake_case"` — `field_name`
- `"camelCase"` — `fieldName`
- `"PascalCase"` — `FieldName`
- `"SCREAMING_SNAKE_CASE"` — `FIELD_NAME`
- `"kebab-case"` — `field-name`
- `"SCREAMING-KEBAB-CASE"` — `FIELD-NAME`
- `"lowercase"` — `fieldname`
- `"UPPERCASE"` — `FIELDNAME`

### `#[serde(rename)]` — Rename individual field

```rust
#[derive(Serialize)]
struct ApiResponse {
    #[serde(rename = "type")]
    response_type: String,

    #[serde(rename = "error_message")]
    error: Option<String>,

    data: serde_json::Value,
}
```

### `#[serde(default)]` — Use Default when missing during deserialization

```rust
#[derive(Serialize, Deserialize, Debug)]
struct Config {
    name: String,

    #[serde(default)]
    verbose: bool,  // defaults to false if missing in JSON

    #[serde(default = "default_timeout")]
    timeout_secs: u64,
}

fn default_timeout() -> u64 { 30 }
```

### `#[serde(flatten)]` — Flatten nested objects

```rust
#[derive(Serialize, Deserialize)]
struct Outer {
    name: String,

    #[serde(flatten)]
    inner: Inner,
}

#[derive(Serialize, Deserialize)]
struct Inner {
    value: i32,
}

// { "name": "test", "value": 42 }
```

### Working with `serde_json::Value`

```rust
use serde_json::{json, Value};

// Create JSON values
let val = json!({
    "model": "whisper-1",
    "file": "audio.wav",
    "language": "en"
});

// Parse from string
let val: Value = serde_json::from_str(r#"{"key": "value"}"#)?;

// Access fields
let model = val["model"].as_str().unwrap_or("unknown");

// Convert to string
let s = serde_json::to_string_pretty(&val)?;
```

### Complete Serialization Example for hermes-video-rs

```rust
use serde::Serialize;

#[derive(Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct TranscriptionRequest {
    pub model: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct TranscriptionResponse {
    pub task: String,
    pub language: String,
    pub duration: f64,
    pub text: String,

    #[serde(default)]
    pub segments: Vec<Segment>,

    #[serde(default)]
    pub words: Vec<Word>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Segment {
    pub id: i32,
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Word {
    pub word: String,
    pub start: f64,
    pub end: f64,
    pub probability: f64,
}

// Usage
let request = TranscriptionRequest {
    model: "whisper-1".to_string(),
    language: Some("en".to_string()),
    prompt: None,
    response_format: Some("verbose_json".to_string()),
    temperature: None,
};

let json = serde_json::to_string_pretty(&request).unwrap();
// {
//   "model": "whisper-1",
//   "language": "en",
//   "response_format": "verbose_json"
// }
```

---

## 4. tempfile

**Crate**: `tempfile 3`  
**Module**: `tempfile`

### Cargo.toml

```toml
tempfile = "3"
```

### Core Concept: RAII Auto-Cleanup

`TempDir` creates a temporary directory that is **automatically deleted** when it
goes out of scope (RAII pattern via `Drop`). You never need to manually delete it.

### `TempDir::new()` — Create temp directory

```rust
pub fn new() -> Result<TempDir>
```

Creates a temporary directory inside `env::temp_dir()` with a randomly generated name.

```rust
use tempfile::TempDir;

fn main() -> std::io::Result<()> {
    let tmp_dir = TempDir::new()?;

    // Use the temporary directory
    let file_path = tmp_dir.path().join("my-temporary-note.txt");
    std::fs::write(&file_path, "Hello from temp!")?;

    // tmp_dir goes out of scope here → directory is auto-deleted
    Ok(())
}
```

### `TempDir::path()` — Get directory path

```rust
pub fn path(&self) -> &Path
```

Access the `Path` to the temporary directory.

```rust
let tmp_dir = TempDir::new()?;
let data_file = tmp_dir.path().join("data.json");
std::fs::write(&data_file, r#"{"key": "value"}"#)?;
// tmp_dir.path() → "/tmp/.tmpXXXXXX/" (random name)
```

### `TempDir::into_path()` / `TempDir::keep()` — Persist directory

```rust
pub fn into_path(self) -> PathBuf  // DEPRECATED — use keep()
pub fn keep(self) -> PathBuf
```

**IMPORTANT**: `into_path()` is **deprecated** in tempfile 3.14+. Use `keep()` instead.

Consumes the `TempDir` **without** deleting the directory. Returns the `PathBuf`
where it is located. The directory will no longer be automatically deleted.

```rust
use tempfile::TempDir;

fn main() -> std::io::Result<()> {
    let tmp_dir = TempDir::new()?;

    // ... create files in tmp_dir ...

    // Persist: prevent auto-deletion, get path for later use
    let persistent_path = tmp_dir.keep();
    // persistent_path is a PathBuf pointing to the directory

    // Now YOU are responsible for cleanup:
    std::fs::remove_dir_all(persistent_path)?;
    Ok(())
}
```

### `TempDir::disable_cleanup()` — In-place disable (testing)

```rust
pub fn disable_cleanup(&mut self, disable_cleanup: bool)
```

Disable cleanup of the temporary directory in-place while keeping the `TempDir` alive.
Primarily useful for testing/debugging.

```rust
let mut tmp_dir = TempDir::new()?;
tmp_dir.disable_cleanup(true); // won't be deleted on drop
// ... inspect files after scope for debugging ...
```

### `TempDir::close()` — Explicit cleanup with error handling

```rust
pub fn close(self) -> Result<()>
```

Closes and removes the temporary directory, returning a `Result`.
Unlike the `Drop` implementation, this reports errors instead of silently ignoring them.

```rust
let tmp_dir = TempDir::new()?;
// ... work with tmp_dir ...
tmp_dir.close()?; // Explicitly delete, propagating any errors
```

### `TempDir::new_in()` — Create in specific parent directory

```rust
pub fn new_in<P: AsRef<Path>>(dir: P) -> Result<TempDir>
```

```rust
let tmp_dir = TempDir::new_in("/data/workspace")?;
```

### `TempDir::with_prefix()` — Create with name prefix

```rust
pub fn with_prefix<S: AsRef<OsStr>>(prefix: S) -> Result<TempDir>
```

```rust
let tmp_dir = TempDir::with_prefix("hermes-video-")?;
// Directory name starts with "hermes-video-"
let name = tmp_dir.path().file_name().unwrap().to_str().unwrap();
assert!(name.starts_with("hermes-video-"));
```

### Builder API

```rust
use tempfile::Builder;

let tmp_dir = Builder::new()
    .prefix("video-processing-")
    .suffix(".work")
    .tempdir()?;

// Or with full control:
let tmp_dir = Builder::new()
    .prefix("hvr-")
    .tempdir_in("/data/workspace")?;
```

### Complete Example for hermes-video-rs

```rust
use tempfile::{Builder, TempDir};
use std::path::Path;

fn process_video(video_path: &Path) -> anyhow::Result<String> {
    // Create a temporary workspace for processing
    let work_dir = Builder::new()
        .prefix("hermes-video-")
        .tempdir()?;

    // Extract audio to temp directory
    let audio_path = work_dir.path().join("audio.wav");
    std::process::Command::new("ffmpeg")
        .args(["-i", video_path.to_str().unwrap()])
        .args(["-vn", "-acodec", "pcm_s16le"])
        .arg(audio_path.to_str().unwrap())
        .status()?;

    // Read audio bytes for upload
    let audio_bytes = std::fs::read(&audio_path)?;

    // work_dir is auto-deleted when this function returns,
    // unless we call keep() to persist it

    Ok(format!("Processed: {} bytes", audio_bytes.len()))
}

// If you need the temp dir to survive the function:
fn extract_audio(video_path: &Path) -> anyhow::Result<(TempDir, std::path::PathBuf)> {
    let work_dir = Builder::new()
        .prefix("hvr-")
        .tempdir()?;

    let audio_path = work_dir.path().join("audio.wav");
    // ... run ffmpeg ...

    // Return both the TempDir and the audio path.
    // Caller decides when to drop the TempDir (triggering cleanup).
    Ok((work_dir, audio_path))
}
```

### Resource Leaking Warning

Platform-specific conditions may cause `TempDir` to fail to delete the directory.
Ensure all file handles (`File`, `ReadDir`) inside the directory are dropped **before**
the `TempDir` goes out of scope.

```rust
// BAD: File handle keeps TempDir locked on Windows
{
    let tmp_dir = TempDir::new()?;
    let mut file = std::fs::File::create(tmp_dir.path().join("test"))?;
    // tmp_dir dropped here, but file handle is still open!
    // On Windows, this can prevent deletion.
}

// GOOD: Drop file first
{
    let tmp_dir = TempDir::new()?;
    {
        let mut file = std::fs::File::create(tmp_dir.path().join("test"))?;
        writeln!(file, "data")?;
    } // file dropped here
} // tmp_dir dropped here — safe to delete
```

---

## 5. which

**Crate**: `which 7`  
**Module**: `which`

### Cargo.toml

```toml
which = "7"
```

### `which::which()` — Find executable in PATH

```rust
pub fn which<T: AsRef<OsStr>>(executable_name: T) -> Result<PathBuf, Error>
```

Locates an installed executable in the system PATH, cross-platform.
Returns the full path to the executable if found.

> **Note**: Returns `Result<PathBuf, which::Error>`, not `Option<PathBuf>`.
> Use `.ok()` to convert to `Option<PathBuf>`.

### Basic Usage

```rust
use which::which;
use std::path::PathBuf;

// Find ffmpeg
let ffmpeg_path = which::which("ffmpeg").unwrap();
// ffmpeg_path = PathBuf::from("/usr/bin/ffmpeg")

// Find deno
let deno_path = which::which("deno").unwrap();

// Check if a command exists (convert to Option)
let result: Option<PathBuf> = which::which("ffmpeg").ok();
match result {
    Some(path) => println!("ffmpeg found at: {}", path.display()),
    None => println!("ffmpeg not found in PATH"),
}

// Handle error directly
match which::which("nonexistent") {
    Ok(path) => println!("Found: {}", path.display()),
    Err(e) => println!("Not found: {}", e),
}
```

### Checking Multiple Commands

```rust
use which::which;

fn check_dependencies() -> Result<(), String> {
    let required = ["ffmpeg", "deno"];
    let mut missing = Vec::new();

    for cmd in &required {
        if which::which(cmd).is_err() {
            missing.push(cmd.to_string());
        }
    }

    if !missing.is_empty() {
        return Err(format!("Missing required tools: {}", missing.join(", ")));
    }

    Ok(())
}

// Usage
match check_dependencies() {
    Ok(()) => println!("All dependencies found"),
    Err(e) => eprintln!("Error: {}", e),
}
```

### Getting Absolute Path

```rust
use which::which;

let path = which::which("python3").unwrap();
println!("Python3 at: {}", path.display());
// /usr/bin/python3

// The returned PathBuf is already an absolute path
assert!(path.is_absolute());
```

### Using `which::which_re` (with `regex` feature)

```rust
// Find all cargo subcommands
#[cfg(feature = "regex")]
fn find_cargo_commands() {
    use which::which_re;
    use regex::Regex;

    let re = Regex::new("^cargo-.*").unwrap();
    let cargo_commands: Vec<_> = which::which_re(re)
        .unwrap()
        .collect();

    for cmd in cargo_commands {
        println!("Found: {}", cmd.display());
    }
}
```

### Complete Example for hermes-video-rs

```rust
use which::which;
use std::path::PathBuf;

pub struct ExternalDeps {
    pub ffmpeg: PathBuf,
    pub deno: Option<PathBuf>,  // optional dependency
}

impl ExternalDeps {
    pub fn detect() -> anyhow::Result<Self> {
        let ffmpeg = which::which("ffmpeg")
            .map_err(|_| anyhow::anyhow!(
                "ffmpeg not found. Install it: sudo apt install ffmpeg"
            ))?;

        let deno = which::which("deno").ok(); // optional

        if deno.is_none() {
            eprintln!("Warning: deno not found. Some features may be unavailable.");
        }

        Ok(Self { ffmpeg, deno })
    }

    pub fn verify_all(&self) -> anyhow::Result<()> {
        println!("ffmpeg: {}", self.ffmpeg.display());
        match &self.deno {
            Some(d) => println!("deno: {}", d.display()),
            None => println!("deno: not installed (optional)"),
        }
        Ok(())
    }
}

// Usage
fn main() -> anyhow::Result<()> {
    let deps = ExternalDeps::detect()?;
    deps.verify_all()?;
    Ok(())
}
```

---

## Quick Reference Cheat Sheet

### reqwest Multipart

```rust
// File upload (async, reads file)
let form = Form::new()
    .text("model", "whisper-1")
    .file("file", "/path/to/audio.wav").await?;

// File upload (bytes, pre-loaded)
let audio = std::fs::read("audio.wav")?;
let part = Part::bytes(audio).file_name("audio.wav").mime_str("audio/wav")?;
let form = Form::new().text("model", "whisper-1").part("file", part);

// Streaming (large files)
let stream = ReaderStream::new(File::open("audio.wav").await?);
let body = Body::wrap_stream(stream);
let part = Part::stream(body).file_name("audio.wav").mime_str("audio/wav")?;
```

### clap Derive

```rust
#[derive(Parser)] struct Cli {
    #[arg(long)]                        flag: bool,           // --flag
    #[arg(long)]                        opt: Option<String>,  // --opt <VALUE> (optional)
    #[arg(short, long, value_enum)]     fmt: OutputFormat,    // --fmt <json|md|both>
    #[arg(default_value_t = 42)]        threads: usize,      // --threads with default
}
```

### serde/serde_json

```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct MyStruct {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    optional: Option<String>,
}

let json = serde_json::to_string_pretty(&my_struct)?;
```

### tempfile

```rust
let tmp = TempDir::new()?;            // auto-deleted on drop
let path = tmp.path().join("file");   // get path
let kept = tmp.keep();                // persist, returns PathBuf (was into_path())
```

### which

```rust
let path: Option<PathBuf> = which::which("ffmpeg").ok();  // None if not found
let path: PathBuf = which::which("ffmpeg")?;              // Error if not found
```
