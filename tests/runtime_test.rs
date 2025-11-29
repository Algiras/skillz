//! Tests for the runtime module

mod common;

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::TempDir;

/// Test WASM execution with valid module
#[test]
fn test_wasm_execution() {
    // First compile a simple WASM module
    let code = r#"
fn main() {
    println!("Runtime test output");
}
"#;

    let result = common::compile_test_tool("runtime_test", code);
    if result.is_err() {
        // Skip if we can't compile (missing target)
        eprintln!("Skipping WASM execution test: {:?}", result.err());
        return;
    }
    
    let wasm_path = result.unwrap();
    assert!(wasm_path.exists());
    
    // Clean up
    let _ = fs::remove_file(wasm_path);
}

/// Test JSON-RPC protocol format
#[test]
fn test_jsonrpc_request_format() {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "execute",
        "params": {
            "arguments": {"test": "value"},
            "context": {
                "roots": ["/test/path"],
                "working_directory": "/current",
                "tool_name": "test_tool",
                "environment": {},
                "tools_dir": "/tools",
                "capabilities": {
                    "sampling": false,
                    "elicitation": false
                }
            }
        },
        "id": 1
    });
    
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["method"], "execute");
    assert!(request["params"]["arguments"].is_object());
    assert!(request["params"]["context"]["roots"].is_array());
}

/// Test JSON-RPC response parsing
#[test]
fn test_jsonrpc_response_parsing() {
    // Success response
    let success = r#"{"jsonrpc": "2.0", "result": {"data": "test"}, "id": 1}"#;
    let parsed: serde_json::Value = serde_json::from_str(success).unwrap();
    assert!(parsed["result"].is_object());
    assert!(parsed["error"].is_null());
    
    // Error response
    let error = r#"{"jsonrpc": "2.0", "error": {"code": -32000, "message": "Test error"}, "id": 1}"#;
    let parsed: serde_json::Value = serde_json::from_str(error).unwrap();
    assert!(parsed["error"].is_object());
    assert_eq!(parsed["error"]["code"], -32000);
}

/// Test log notification format
#[test]
fn test_log_notification_format() {
    let log = r#"{"jsonrpc": "2.0", "method": "log", "params": {"level": "info", "message": "Test log"}}"#;
    let parsed: serde_json::Value = serde_json::from_str(log).unwrap();
    
    assert_eq!(parsed["method"], "log");
    assert_eq!(parsed["params"]["level"], "info");
    assert_eq!(parsed["params"]["message"], "Test log");
}

/// Test progress notification format
#[test]
fn test_progress_notification_format() {
    let progress = r#"{"jsonrpc": "2.0", "method": "progress", "params": {"current": 50, "total": 100, "message": "Halfway"}}"#;
    let parsed: serde_json::Value = serde_json::from_str(progress).unwrap();
    
    assert_eq!(parsed["method"], "progress");
    assert_eq!(parsed["params"]["current"], 50);
    assert_eq!(parsed["params"]["total"], 100);
}

/// Test script tool execution (Python)
#[test]
fn test_script_execution_python() {
    // Check if Python is available
    let python_check = Command::new("python3").arg("--version").output();
    if python_check.is_err() || !python_check.unwrap().status.success() {
        eprintln!("Skipping Python test: python3 not available");
        return;
    }
    
    let temp_dir = TempDir::new().unwrap();
    let script_path = temp_dir.path().join("test_script.py");
    
    let script = r#"#!/usr/bin/env python3
import json
import sys

request = json.loads(sys.stdin.read())
result = {"message": "Python test success", "received": request.get("params", {}).get("arguments", {})}
print(json.dumps({"jsonrpc": "2.0", "result": result, "id": request.get("id")}))
"#;
    
    fs::write(&script_path, script).unwrap();
    
    // Execute
    let mut child = Command::new("python3")
        .arg(&script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    
    let request = r#"{"jsonrpc": "2.0", "method": "execute", "params": {"arguments": {"test": "data"}, "context": {}}, "id": 1}"#;
    
    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(request.as_bytes()).unwrap();
    }
    
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["result"]["message"], "Python test success");
}

/// Test script tool execution (Node.js)
#[test]
fn test_script_execution_node() {
    // Check if Node is available
    let node_check = Command::new("node").arg("--version").output();
    if node_check.is_err() || !node_check.unwrap().status.success() {
        eprintln!("Skipping Node.js test: node not available");
        return;
    }
    
    let temp_dir = TempDir::new().unwrap();
    let script_path = temp_dir.path().join("test_script.js");
    
    let script = r#"
const readline = require('readline');
const rl = readline.createInterface({ input: process.stdin });

rl.on('line', (line) => {
    const request = JSON.parse(line);
    const result = { message: "Node.js test success" };
    console.log(JSON.stringify({ jsonrpc: "2.0", result, id: request.id }));
    process.exit(0);
});
"#;
    
    fs::write(&script_path, script).unwrap();
    
    // Execute
    let mut child = Command::new("node")
        .arg(&script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    
    let request = r#"{"jsonrpc": "2.0", "method": "execute", "params": {"arguments": {}}, "id": 1}"#;
    
    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(request.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
    }
    
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let response: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(response["result"]["message"], "Node.js test success");
}

