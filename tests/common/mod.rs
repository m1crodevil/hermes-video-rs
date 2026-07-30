pub struct TestContext {
    pub temp_dir: tempfile::TempDir,
}

impl TestContext {
    pub fn new() -> Self {
        Self {
            temp_dir: tempfile::TempDir::new().unwrap(),
        }
    }

    pub fn fixture_path(&self, name: &str) -> std::path::PathBuf {
        self.temp_dir.path().join(name)
    }
}
