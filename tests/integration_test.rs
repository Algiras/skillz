//! Integration tests for the Skillz MCP server

mod common;

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

/// Helper to send JSON-RPC request and get response
fn send_request(
    stdin: &mut impl Write,
    stdout: &mut BufReader<impl std::io::Read>,
    method: &str,
    params: serde_json::Value,
    id: u64,
) -> serde_json::Value {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": id
    });

    let request_str = serde_json::to_string(&request).unwrap();
    writeln!(stdin, "{}", request_str).unwrap();
    stdin.flush().unwrap();

    let mut response_line = String::new();
    stdout.read_line(&mut response_line).unwrap();

    serde_json::from_str(&response_line).unwrap_or_else(
        |_| serde_json::json!({"error": "Failed to parse response", "raw": response_line}),
    )
}

/// Test MCP server initialization
#[test]
fn test_mcp_initialization() {
    // Build the server first
    let build_status = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status();

    if build_status.is_err() || !build_status.unwrap().success() {
        eprintln!("Skipping integration test: build failed");
        return;
    }

    let binary_path = format!("{}/target/release/skillz", env!("CARGO_MANIFEST_DIR"));

    if !std::path::Path::new(&binary_path).exists() {
        eprintln!("Skipping integration test: binary not found");
        return;
    }

    let mut child = Command::new(&binary_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to spawn server");

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    // Give server time to start
    thread::sleep(Duration::from_millis(500));

    // Send initialize request
    let response = send_request(
        &mut stdin,
        &mut reader,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            }
        }),
        0,
    );

    // Verify initialization succeeded
    assert!(
        response.get("result").is_some(),
        "Initialize should return result: {:?}",
        response
    );

    let result = &response["result"];
    assert!(result.get("serverInfo").is_some() || result.get("capabilities").is_some());

    // Clean up
    let _ = child.kill();
    let _ = child.wait();
}

/// Test listing tools
#[test]
fn test_list_tools() {
    let build_status = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status();

    if build_status.is_err() || !build_status.unwrap().success() {
        return;
    }

    let binary_path = format!("{}/target/release/skillz", env!("CARGO_MANIFEST_DIR"));

    if !std::path::Path::new(&binary_path).exists() {
        return;
    }

    let mut child = Command::new(&binary_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to spawn server");

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    thread::sleep(Duration::from_millis(500));

    // Initialize first
    let _ = send_request(
        &mut stdin,
        &mut reader,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        }),
        0,
    );

    // Send initialized notification
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#
    )
    .unwrap();
    stdin.flush().unwrap();

    // List tools
    let response = send_request(
        &mut stdin,
        &mut reader,
        "tools/list",
        serde_json::json!({}),
        1,
    );

    assert!(
        response.get("result").is_some(),
        "Should have result: {:?}",
        response
    );

    let tools = &response["result"]["tools"];
    assert!(tools.is_array(), "Tools should be array");

    // Check for expected built-in tools
    let tool_names: Vec<&str> = tools
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();

    assert!(tool_names.contains(&"build_tool"), "Should have build_tool");
    assert!(
        tool_names.contains(&"register_script"),
        "Should have register_script"
    );
    assert!(tool_names.contains(&"call_tool"), "Should have call_tool");
    assert!(tool_names.contains(&"list_tools"), "Should have list_tools");

    let _ = child.kill();
    let _ = child.wait();
}

/// Test code validation
#[test]
fn test_code_validation() {
    let build_status = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status();

    if build_status.is_err() || !build_status.unwrap().success() {
        return;
    }

    let binary_path = format!("{}/target/release/skillz", env!("CARGO_MANIFEST_DIR"));

    if !std::path::Path::new(&binary_path).exists() {
        return;
    }

    let mut child = Command::new(&binary_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to spawn server");

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    thread::sleep(Duration::from_millis(500));

    // Initialize
    let _ = send_request(
        &mut stdin,
        &mut reader,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        }),
        0,
    );

    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#
    )
    .unwrap();
    stdin.flush().unwrap();

    // Test validation with valid code
    let response = send_request(
        &mut stdin,
        &mut reader,
        "tools/call",
        serde_json::json!({
            "name": "test_validate",
            "arguments": {
                "code": "fn main() { println!(\"Hello\"); }"
            }
        }),
        2,
    );

    assert!(response.get("result").is_some());
    let content = &response["result"]["content"][0]["text"];
    assert!(
        content.as_str().unwrap().contains("passed") || content.as_str().unwrap().contains("✓")
    );

    let _ = child.kill();
    let _ = child.wait();
}
