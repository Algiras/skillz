//! Tests for the tool registry

use tempfile::TempDir;

// ==================== Basic Operations ====================

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

// ==================== New Directory Structure Tests ====================

mod directory_structure {
    use super::*;

    /// Test creating tool directory with manifest
    #[test]
    fn test_tool_directory_structure() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let tool_name = "my_tool";
        let tool_dir = temp_dir.path().join(tool_name);

        // Create tool directory
        std::fs::create_dir_all(&tool_dir).expect("Failed to create tool dir");

        // Create manifest.json
        let manifest = serde_json::json!({
            "name": tool_name,
            "version": "1.0.0",
            "description": "A test tool",
            "tool_type": "wasm",
            "wasm_dependencies": [],
            "dependencies": [],
            "tags": []
        });

        let manifest_path = tool_dir.join("manifest.json");
        std::fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .expect("Failed to write manifest");

        // Verify structure
        assert!(tool_dir.exists());
        assert!(manifest_path.exists());

        // Read and verify manifest
        let content = std::fs::read_to_string(&manifest_path).expect("Failed to read");
        let parsed: serde_json::Value = serde_json::from_str(&content).expect("Failed to parse");
        assert_eq!(parsed["name"], tool_name);
        assert_eq!(parsed["version"], "1.0.0");
    }

    /// Test WASM tool with source code
    #[test]
    fn test_wasm_tool_with_source() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let tool_name = "wasm_tool";
        let tool_dir = temp_dir.path().join(tool_name);

        std::fs::create_dir_all(&tool_dir).expect("Failed to create tool dir");

        // Create manifest
        let manifest = serde_json::json!({
            "name": tool_name,
            "version": "1.0.0",
            "description": "WASM tool with source",
            "tool_type": "wasm",
            "wasm_dependencies": ["serde@1.0[derive]"]
        });

        std::fs::write(
            tool_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .expect("Failed to write manifest");

        // Create source file
        let source = r#"fn main() { println!("Hello!"); }"#;
        std::fs::write(tool_dir.join("src.rs"), source).expect("Failed to write source");

        // Create mock WASM file
        let wasm_content = vec![0x00, 0x61, 0x73, 0x6D]; // WASM magic number
        std::fs::write(tool_dir.join(format!("{}.wasm", tool_name)), wasm_content)
            .expect("Failed to write WASM");

        // Verify all files exist
        assert!(tool_dir.join("manifest.json").exists());
        assert!(tool_dir.join("src.rs").exists());
        assert!(tool_dir.join(format!("{}.wasm", tool_name)).exists());
    }

    /// Test script tool directory
    #[test]
    fn test_script_tool_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let tool_name = "script_tool";
        let tool_dir = temp_dir.path().join(tool_name);

        std::fs::create_dir_all(&tool_dir).expect("Failed to create tool dir");

        // Create manifest
        let manifest = serde_json::json!({
            "name": tool_name,
            "version": "1.0.0",
            "description": "Script tool",
            "tool_type": "script",
            "interpreter": "python3",
            "dependencies": ["requests", "pandas"]
        });

        std::fs::write(
            tool_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .expect("Failed to write manifest");

        // Create script file
        let script = r#"#!/usr/bin/env python3
import json, sys
print(json.dumps({"result": "ok"}))"#;
        std::fs::write(tool_dir.join(format!("{}.py", tool_name)), script)
            .expect("Failed to write script");

        // Verify
        assert!(tool_dir.join("manifest.json").exists());
        assert!(tool_dir.join(format!("{}.py", tool_name)).exists());
    }

    /// Test manifest with all fields
    #[test]
    fn test_full_manifest() {
        let manifest = serde_json::json!({
            "name": "full_tool",
            "version": "2.1.0",
            "description": "A fully specified tool",
            "tool_type": "wasm",
            "interpreter": null,
            "input_schema": {
                "type": "object",
                "properties": {
                    "name": {"type": "string"}
                },
                "required": ["name"]
            },
            "output_schema": {
                "type": "object",
                "properties": {
                    "result": {"type": "string"}
                }
            },
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false
            },
            "dependencies": [],
            "wasm_dependencies": ["serde@1.0[derive]", "anyhow@1.0"],
            "author": "Test Author",
            "license": "MIT",
            "repository": "https://github.com/test/tool",
            "tags": ["utility", "example"],
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-02T00:00:00Z"
        });

        // Verify all fields are serializable
        let json_str = serde_json::to_string_pretty(&manifest).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed["name"], "full_tool");
        assert_eq!(parsed["version"], "2.1.0");
        assert_eq!(parsed["author"], "Test Author");
        assert!(parsed["wasm_dependencies"].is_array());
        assert_eq!(parsed["wasm_dependencies"].as_array().unwrap().len(), 2);
    }
}

// ==================== Versioning Tests ====================

mod versioning {
    use super::*;
    use skillz::registry::{ToolManifest, ToolRegistry, ToolType};

    /// Test version auto-increment
    #[test]
    fn test_version_increment() {
        assert_eq!(ToolRegistry::increment_version("1.0.0"), "1.0.1");
        assert_eq!(ToolRegistry::increment_version("1.0.9"), "1.0.10");
        assert_eq!(ToolRegistry::increment_version("2.5.3"), "2.5.4");
        assert_eq!(ToolRegistry::increment_version("0.1.0"), "0.1.1");
    }

    /// Test version backup and listing
    #[test]
    fn test_version_backup_and_list() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let registry = ToolRegistry::new(temp_dir.path().to_path_buf());

        // Create initial tool
        let manifest = ToolManifest::new(
            "test_versioned".to_string(),
            "A versioned tool".to_string(),
            ToolType::Script,
        );

        let script_code = b"#!/usr/bin/env python3\nprint('v1')";
        registry
            .register_tool(manifest, script_code)
            .expect("Failed to register tool");

        // Verify initial version
        let tool = registry.get_tool("test_versioned").unwrap();
        assert_eq!(tool.manifest.version, "1.0.0");

        // Update tool (should auto-backup and increment)
        let manifest2 = ToolManifest::new(
            "test_versioned".to_string(),
            "Updated versioned tool".to_string(),
            ToolType::Script,
        );

        let script_code2 = b"#!/usr/bin/env python3\nprint('v2')";
        registry
            .register_tool(manifest2, script_code2)
            .expect("Failed to register updated tool");

        // Verify version was incremented
        let tool = registry.get_tool("test_versioned").unwrap();
        assert_eq!(tool.manifest.version, "1.0.1");

        // Check versions list
        let versions = registry.list_versions("test_versioned").unwrap();
        assert!(versions.len() >= 2); // Current + at least one backup
        assert!(versions
            .iter()
            .any(|v| v.contains("1.0.1") && v.contains("current")));
        assert!(versions.iter().any(|v| v.starts_with("1.0.0")));
    }

    /// Test version rollback
    #[test]
    fn test_version_rollback() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let registry = ToolRegistry::new(temp_dir.path().to_path_buf());

        // Create initial tool
        let mut manifest = ToolManifest::new(
            "rollback_test".to_string(),
            "Original description".to_string(),
            ToolType::Script,
        );
        manifest.interpreter = Some("python3".to_string());

        let script_code = b"#!/usr/bin/env python3\nprint('original')";
        registry
            .register_tool(manifest, script_code)
            .expect("Failed to register tool");

        // Update tool
        let mut manifest2 = ToolManifest::new(
            "rollback_test".to_string(),
            "Updated description".to_string(),
            ToolType::Script,
        );
        manifest2.interpreter = Some("python3".to_string());

        let script_code2 = b"#!/usr/bin/env python3\nprint('updated')";
        registry
            .register_tool(manifest2, script_code2)
            .expect("Failed to update tool");

        // Verify updated version
        let tool = registry.get_tool("rollback_test").unwrap();
        assert_eq!(tool.manifest.version, "1.0.1");
        assert_eq!(tool.manifest.description, "Updated description");

        // Rollback to 1.0.0
        registry
            .rollback("rollback_test", "1.0.0")
            .expect("Failed to rollback");

        // Verify rollback
        let tool = registry.get_tool("rollback_test").unwrap();
        assert_eq!(tool.manifest.version, "1.0.0");
        assert_eq!(tool.manifest.description, "Original description");
    }

    /// Test versions directory structure
    #[test]
    fn test_versions_directory_structure() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let tool_name = "versioned_tool";
        let tool_dir = temp_dir.path().join(tool_name);
        let versions_dir = tool_dir.join("versions");
        let v1_dir = versions_dir.join("1.0.0");

        // Create directory structure
        std::fs::create_dir_all(&v1_dir).expect("Failed to create version dir");

        // Create version manifest
        let manifest = serde_json::json!({
            "name": tool_name,
            "version": "1.0.0",
            "description": "Version 1.0.0",
            "tool_type": "script"
        });

        std::fs::write(
            v1_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .expect("Failed to write manifest");

        // Verify structure
        assert!(versions_dir.exists());
        assert!(v1_dir.exists());
        assert!(v1_dir.join("manifest.json").exists());
    }
}
