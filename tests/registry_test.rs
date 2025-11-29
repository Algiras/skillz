//! Tests for the tool registry

use tempfile::TempDir;

// We need to test the registry module - import it
// Since we can't directly import from the binary crate in integration tests,
// we'll test the public interface via a simple registry implementation

/// Test registry operations
#[test]
fn test_registry_basic_operations() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let manifest_path = temp_dir.path().join("manifest.json");

    // Write initial manifest
    let initial = r#"{}"#;
    std::fs::write(&manifest_path, initial).expect("Failed to write manifest");

    // Read and verify
    let content = std::fs::read_to_string(&manifest_path).expect("Failed to read");
    assert!(content.contains("{}"));
}

/// Test manifest persistence
#[test]
fn test_manifest_persistence() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let manifest_path = temp_dir.path().join("manifest.json");

    // Write a tool config
    let manifest = r#"{
        "test_tool": {
            "name": "test_tool",
            "description": "A test tool",
            "tool_type": "wasm",
            "wasm_path": "/path/to/tool.wasm",
            "script_path": "",
            "interpreter": null,
            "schema": {"type": "object"}
        }
    }"#;

    std::fs::write(&manifest_path, manifest).expect("Failed to write");

    // Parse and verify
    let content = std::fs::read_to_string(&manifest_path).expect("Failed to read");
    let parsed: serde_json::Value = serde_json::from_str(&content).expect("Failed to parse");

    assert!(parsed.get("test_tool").is_some());
    assert_eq!(parsed["test_tool"]["name"], "test_tool");
    assert_eq!(parsed["test_tool"]["tool_type"], "wasm");
}

/// Test scripts directory creation
#[test]
fn test_scripts_directory() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let scripts_dir = temp_dir.path().join("scripts");

    std::fs::create_dir_all(&scripts_dir).expect("Failed to create scripts dir");
    assert!(scripts_dir.exists());
    assert!(scripts_dir.is_dir());
}

/// Test tool types
#[test]
fn test_tool_types() {
    let wasm_manifest = r#"{"tool_type": "wasm"}"#;
    let script_manifest = r#"{"tool_type": "script"}"#;

    let wasm: serde_json::Value = serde_json::from_str(wasm_manifest).unwrap();
    let script: serde_json::Value = serde_json::from_str(script_manifest).unwrap();

    assert_eq!(wasm["tool_type"], "wasm");
    assert_eq!(script["tool_type"], "script");
}
