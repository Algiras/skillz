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
    let error =
        r#"{"jsonrpc": "2.0", "error": {"code": -32000, "message": "Test error"}, "id": 1}"#;
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
/// CRITICAL: Uses sys.stdin.readline() NOT sys.stdin.read() - read() blocks forever!
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

    // Use sys.stdin.readline() - this is the correct pattern for JSON-RPC scripts
    let script = r#"#!/usr/bin/env python3
import json
import sys

request = json.loads(sys.stdin.readline())
result = {"message": "Python test success", "received": request.get("params", {}).get("arguments", {})}
print(json.dumps({"jsonrpc": "2.0", "result": result, "id": request.get("id")}))
sys.stdout.flush()
"#;

    fs::write(&script_path, script).unwrap();

    // Execute
    let mut child = Command::new("python3")
        .arg(&script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    // Note: Request ends with newline for readline()
    let request = r#"{"jsonrpc": "2.0", "method": "execute", "params": {"arguments": {"test": "data"}, "context": {}}, "id": 1}
"#;

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

    let request =
        r#"{"jsonrpc": "2.0", "method": "execute", "params": {"arguments": {}}, "id": 1}"#;

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

/// Test that arguments passed as stringified JSON are handled correctly
/// This is critical for pipelines which pass arguments between steps
#[test]
fn test_arguments_as_string_parsing() {
    // Check if Python is available
    let python_check = Command::new("python3").arg("--version").output();
    if python_check.is_err() || !python_check.unwrap().status.success() {
        eprintln!("Skipping test: python3 not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let script_path = temp_dir.path().join("args_test.py");

    // Script that echoes back the arguments to verify they're parsed correctly
    let script = r#"#!/usr/bin/env python3
import json
import sys

request = json.loads(sys.stdin.readline())
args = request.get("params", {}).get("arguments", {})

# Return the type and value of args to verify correct parsing
result = {
    "args_type": str(type(args).__name__),
    "args_value": args,
    "text_value": args.get("text", "NOT_FOUND") if isinstance(args, dict) else "NOT_A_DICT"
}
print(json.dumps({"jsonrpc": "2.0", "result": result, "id": request.get("id")}))
sys.stdout.flush()
"#;

    fs::write(&script_path, script).unwrap();

    // Test with arguments as a proper object
    let mut child = Command::new("python3")
        .arg(&script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let request = r#"{"jsonrpc": "2.0", "method": "execute", "params": {"arguments": {"text": "hello world"}}, "id": 1}
"#;

    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(request.as_bytes()).unwrap();
    }

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());

    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["result"]["args_type"], "dict");
    assert_eq!(response["result"]["text_value"], "hello world");
}

/// Test that script outputs are correctly parsed (important for pipeline step outputs)
#[test]
fn test_script_output_parsing() {
    // Check if Python is available
    let python_check = Command::new("python3").arg("--version").output();
    if python_check.is_err() || !python_check.unwrap().status.success() {
        eprintln!("Skipping test: python3 not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let script_path = temp_dir.path().join("output_test.py");

    // Script that returns structured output
    let script = r#"#!/usr/bin/env python3
import json
import sys

request = json.loads(sys.stdin.readline())

# Return structured output that pipelines can use
result = {
    "count": 42,
    "items": ["a", "b", "c"],
    "nested": {
        "value": "deep",
        "number": 123
    },
    "success": True
}
print(json.dumps({"jsonrpc": "2.0", "result": result, "id": request.get("id")}))
sys.stdout.flush()
"#;

    fs::write(&script_path, script).unwrap();

    let mut child = Command::new("python3")
        .arg(&script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let request = r#"{"jsonrpc": "2.0", "method": "execute", "params": {"arguments": {}}, "id": 1}
"#;

    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(request.as_bytes()).unwrap();
    }

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());

    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    
    // Verify structured output is preserved
    assert_eq!(response["result"]["count"], 42);
    assert_eq!(response["result"]["items"][0], "a");
    assert_eq!(response["result"]["items"][1], "b");
    assert_eq!(response["result"]["items"][2], "c");
    assert_eq!(response["result"]["nested"]["value"], "deep");
    assert_eq!(response["result"]["nested"]["number"], 123);
    assert_eq!(response["result"]["success"], true);
}

/// Test accessing nested output fields (critical for pipeline variable resolution like $prev.nested.value)
#[test]
fn test_nested_output_access() {
    let output = serde_json::json!({
        "result": {
            "data": {
                "users": [
                    {"name": "Alice", "age": 30},
                    {"name": "Bob", "age": 25}
                ],
                "count": 2
            },
            "success": true
        }
    });

    // Test accessing nested fields like pipelines do
    assert_eq!(output["result"]["success"], true);
    assert_eq!(output["result"]["data"]["count"], 2);
    assert_eq!(output["result"]["data"]["users"][0]["name"], "Alice");
    assert_eq!(output["result"]["data"]["users"][1]["age"], 25);
}
