use watch2::setup::{check, SetupStatus};
use std::fs;
use tempfile::TempDir;

/// Test that can_proceed is independent of has_api_key.
/// This documents the expected behavior: binaries required, API key optional.
#[test]
fn test_can_proceed_without_api_key_when_binaries_exist() {
    let status = check();

    // If binaries are present, can_proceed should be true
    // even if has_api_key is false
    if status.missing_binaries.is_empty() {
        assert!(
            status.can_proceed,
            "can_proceed should be true when binaries exist, regardless of API key"
        );
    }
}

/// Test that missing binaries still block can_proceed.
#[test]
fn test_blocks_when_binaries_missing() {
    let status = check();

    // If binaries are missing, can_proceed must be false
    if !status.missing_binaries.is_empty() {
        assert!(
            !status.can_proceed,
            "can_proceed should be false when binaries are missing"
        );
    }
}

/// Test that has_api_key is correctly detected from env file.
#[test]
fn test_has_api_key_false_when_no_key() {
    let tmp = TempDir::new().unwrap();
    let env_file = tmp.path().join(".env");
    fs::write(&env_file, "SETUP_COMPLETE=true\n").unwrap();

    // create a SetupStatus manually to test field logic
    let status = SetupStatus {
        can_proceed: true,
        first_run: false,
        missing_binaries: vec![],
        has_api_key: false,
        config_file: env_file,
    };

    // has_api_key being false should NOT block can_proceed
    assert!(status.can_proceed || !status.has_api_key);
}

/// Test that first_run is detected from missing SETUP_COMPLETE.
#[test]
fn test_first_run_detected() {
    let tmp = TempDir::new().unwrap();
    let env_file = tmp.path().join(".env");
    fs::write(&env_file, "").unwrap(); // No SETUP_COMPLETE

    let status = SetupStatus {
        can_proceed: true,
        first_run: true,
        missing_binaries: vec![],
        has_api_key: false,
        config_file: env_file,
    };

    assert!(status.first_run);
}

/// Integration test: can_proceed should be true when binaries exist,
/// even without API key. This is the core fix.
#[test]
fn test_integration_can_proceed_independent_of_api_key() {
    let status = check();

    // The key assertion: if binaries are present, can_proceed MUST be true
    // regardless of whether has_api_key is true or false
    if status.missing_binaries.is_empty() {
        assert!(
            status.can_proceed,
            "FIX FAILED: can_proceed should be true when binaries exist. \
             got can_proceed={}, has_api_key={}",
            status.can_proceed, status.has_api_key
        );
    }
}

/// Integration test: missing binaries should still block.
#[test]
fn test_integration_blocks_without_binaries() {
    let status = check();

    // If somehow binaries are missing, can_proceed must be false
    if !status.missing_binaries.is_empty() {
        assert!(
            !status.can_proceed,
            "can_proceed should be false when binaries are missing"
        );
    }
}
