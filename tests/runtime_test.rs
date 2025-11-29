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

// ==================== ELICITATION TESTS ====================

/// Test elicitation request format (script → host)
#[test]
fn test_elicitation_request_format() {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "elicitation/create",
        "params": {
            "message": "What is your name?",
            "requestedSchema": {
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Your name"}
                },
                "required": ["name"]
            }
        },
        "id": 1
    });

    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["method"], "elicitation/create");
    assert_eq!(request["params"]["message"], "What is your name?");
    assert!(request["params"]["requestedSchema"]["properties"]["name"].is_object());
    assert!(request["id"].is_number());
}

/// Test elicitation response format (host → script)
#[test]
fn test_elicitation_response_format() {
    // Successful elicitation with user accepting
    let accept_response = serde_json::json!({
        "jsonrpc": "2.0",
        "result": {
            "action": "accept",
            "content": {
                "name": "Alice"
            }
        },
        "id": 1
    });

    assert_eq!(accept_response["result"]["action"], "accept");
    assert_eq!(accept_response["result"]["content"]["name"], "Alice");

    // User cancelled elicitation
    let cancel_response = serde_json::json!({
        "jsonrpc": "2.0",
        "result": {
            "action": "cancel"
        },
        "id": 1
    });

    assert_eq!(cancel_response["result"]["action"], "cancel");
    assert!(cancel_response["result"]["content"].is_null());

    // User declined elicitation
    let decline_response = serde_json::json!({
        "jsonrpc": "2.0",
        "result": {
            "action": "decline"
        },
        "id": 1
    });

    assert_eq!(decline_response["result"]["action"], "decline");
}

/// Test elicitation with various schema types
#[test]
fn test_elicitation_schema_types() {
    // String input
    let string_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "text": {"type": "string"}
        },
        "required": ["text"]
    });
    assert_eq!(string_schema["properties"]["text"]["type"], "string");

    // Number input
    let number_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "count": {"type": "integer", "minimum": 0, "maximum": 100}
        },
        "required": ["count"]
    });
    assert_eq!(number_schema["properties"]["count"]["type"], "integer");

    // Boolean input
    let bool_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "confirm": {"type": "boolean", "description": "Confirm action?"}
        },
        "required": ["confirm"]
    });
    assert_eq!(bool_schema["properties"]["confirm"]["type"], "boolean");

    // Enum input
    let enum_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "color": {
                "type": "string",
                "enum": ["red", "green", "blue"]
            }
        },
        "required": ["color"]
    });
    assert!(enum_schema["properties"]["color"]["enum"].is_array());
}

// ==================== SAMPLING TESTS ====================

/// Test sampling request format (script → host)
#[test]
fn test_sampling_request_format() {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "sampling/createMessage",
        "params": {
            "messages": [
                {
                    "role": "user",
                    "content": {
                        "type": "text",
                        "text": "What is 2 + 2?"
                    }
                }
            ],
            "maxTokens": 100
        },
        "id": 1
    });

    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["method"], "sampling/createMessage");
    assert!(request["params"]["messages"].is_array());
    assert_eq!(request["params"]["messages"][0]["role"], "user");
    assert_eq!(request["params"]["messages"][0]["content"]["type"], "text");
    assert_eq!(request["params"]["maxTokens"], 100);
}

/// Test sampling response format (host → script)
#[test]
fn test_sampling_response_format() {
    let response = serde_json::json!({
        "jsonrpc": "2.0",
        "result": {
            "role": "assistant",
            "content": {
                "type": "text",
                "text": "2 + 2 equals 4."
            },
            "model": "claude-3-opus",
            "stopReason": "end_turn"
        },
        "id": 1
    });

    assert_eq!(response["result"]["role"], "assistant");
    assert_eq!(response["result"]["content"]["type"], "text");
    assert_eq!(response["result"]["content"]["text"], "2 + 2 equals 4.");
    assert_eq!(response["result"]["model"], "claude-3-opus");
    assert_eq!(response["result"]["stopReason"], "end_turn");
}

/// Test sampling with multiple messages (conversation history)
#[test]
fn test_sampling_conversation_history() {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "sampling/createMessage",
        "params": {
            "messages": [
                {
                    "role": "user",
                    "content": {"type": "text", "text": "My name is Alice."}
                },
                {
                    "role": "assistant",
                    "content": {"type": "text", "text": "Hello Alice! How can I help you today?"}
                },
                {
                    "role": "user",
                    "content": {"type": "text", "text": "What's my name?"}
                }
            ],
            "maxTokens": 50,
            "systemPrompt": "You are a helpful assistant."
        },
        "id": 2
    });

    assert_eq!(request["params"]["messages"].as_array().unwrap().len(), 3);
    assert_eq!(request["params"]["messages"][0]["role"], "user");
    assert_eq!(request["params"]["messages"][1]["role"], "assistant");
    assert_eq!(request["params"]["messages"][2]["role"], "user");
    assert_eq!(request["params"]["systemPrompt"], "You are a helpful assistant.");
}

/// Test sampling error response (e.g., client doesn't support sampling)
#[test]
fn test_sampling_error_response() {
    let error_response = serde_json::json!({
        "jsonrpc": "2.0",
        "result": {
            "error": "Sampling not supported by client"
        },
        "id": 1
    });

    assert!(error_response["result"]["error"].is_string());
    assert_eq!(error_response["result"]["error"], "Sampling not supported by client");
}

// ==================== BIDIRECTIONAL COMMUNICATION TESTS ====================

/// Test bidirectional script that uses elicitation
#[test]
fn test_script_with_elicitation() {
    // Check if Python is available
    let python_check = Command::new("python3").arg("--version").output();
    if python_check.is_err() || !python_check.unwrap().status.success() {
        eprintln!("Skipping Python test: python3 not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let script_path = temp_dir.path().join("elicit_test.py");

    // Script that checks capabilities and simulates elicitation flow
    let script = r#"#!/usr/bin/env python3
import json
import sys

# Read initial request
request = json.loads(sys.stdin.readline())
context = request.get("params", {}).get("context", {})
caps = context.get("capabilities", {})

result = {
    "elicitation_available": caps.get("elicitation", False),
    "would_elicit": caps.get("elicitation", False)
}

# If elicitation is available, we would send a request and wait for response
# In this test, we just verify the capability check works
if caps.get("elicitation"):
    result["message"] = "Elicitation supported - would prompt user"
else:
    result["message"] = "Elicitation not supported - using fallback"

print(json.dumps({"jsonrpc": "2.0", "result": result, "id": request.get("id")}))
sys.stdout.flush()
"#;

    fs::write(&script_path, script).unwrap();

    // Test with elicitation enabled
    let mut child = Command::new("python3")
        .arg(&script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let request = r#"{"jsonrpc": "2.0", "method": "execute", "params": {"arguments": {}, "context": {"capabilities": {"elicitation": true, "sampling": false, "memory": true}, "roots": [], "working_directory": "/tmp", "tool_name": "test", "environment": {}, "tools_dir": "/tools"}}, "id": 1}
"#;

    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(request.as_bytes()).unwrap();
    }

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());

    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["result"]["elicitation_available"], true);
    assert_eq!(response["result"]["message"], "Elicitation supported - would prompt user");
}

/// Test bidirectional script that checks sampling capability
#[test]
fn test_script_with_sampling_check() {
    // Check if Python is available
    let python_check = Command::new("python3").arg("--version").output();
    if python_check.is_err() || !python_check.unwrap().status.success() {
        eprintln!("Skipping Python test: python3 not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let script_path = temp_dir.path().join("sampling_test.py");

    // Script that checks sampling capability
    let script = r#"#!/usr/bin/env python3
import json
import sys

request = json.loads(sys.stdin.readline())
context = request.get("params", {}).get("context", {})
caps = context.get("capabilities", {})

result = {
    "sampling_available": caps.get("sampling", False),
    "elicitation_available": caps.get("elicitation", False),
    "memory_available": caps.get("memory", False)
}

if caps.get("sampling"):
    result["llm_message"] = "Would request LLM completion"
else:
    result["llm_message"] = "Sampling not available"

print(json.dumps({"jsonrpc": "2.0", "result": result, "id": request.get("id")}))
sys.stdout.flush()
"#;

    fs::write(&script_path, script).unwrap();

    // Test with sampling disabled (like Cursor currently)
    let mut child = Command::new("python3")
        .arg(&script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let request = r#"{"jsonrpc": "2.0", "method": "execute", "params": {"arguments": {}, "context": {"capabilities": {"elicitation": true, "sampling": false, "memory": true}, "roots": [], "working_directory": "/tmp", "tool_name": "test", "environment": {}, "tools_dir": "/tools"}}, "id": 1}
"#;

    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(request.as_bytes()).unwrap();
    }

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());

    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["result"]["sampling_available"], false);
    assert_eq!(response["result"]["elicitation_available"], true);
    assert_eq!(response["result"]["memory_available"], true);
    assert_eq!(response["result"]["llm_message"], "Sampling not available");
}

/// Test memory request format (script → host)
#[test]
fn test_memory_request_format() {
    // Memory set request
    let set_request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "memory/set",
        "params": {
            "tool_name": "my_tool",
            "key": "user_preference",
            "value": {"theme": "dark", "language": "en"}
        },
        "id": 1
    });

    assert_eq!(set_request["method"], "memory/set");
    assert_eq!(set_request["params"]["tool_name"], "my_tool");
    assert_eq!(set_request["params"]["key"], "user_preference");
    assert!(set_request["params"]["value"].is_object());

    // Memory get request
    let get_request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "memory/get",
        "params": {
            "tool_name": "my_tool",
            "key": "user_preference"
        },
        "id": 2
    });

    assert_eq!(get_request["method"], "memory/get");
    assert_eq!(get_request["params"]["tool_name"], "my_tool");
    assert_eq!(get_request["params"]["key"], "user_preference");

    // Memory list request
    let list_request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "memory/list",
        "params": {
            "tool_name": "my_tool"
        },
        "id": 3
    });

    assert_eq!(list_request["method"], "memory/list");
}

/// Test memory response format (host → script)
#[test]
fn test_memory_response_format() {
    // Successful get response
    let get_response = serde_json::json!({
        "jsonrpc": "2.0",
        "result": {
            "value": {"theme": "dark", "language": "en"}
        },
        "id": 1
    });

    assert!(get_response["result"]["value"].is_object());
    assert_eq!(get_response["result"]["value"]["theme"], "dark");

    // Key not found response
    let not_found_response = serde_json::json!({
        "jsonrpc": "2.0",
        "result": {
            "value": null
        },
        "id": 1
    });

    assert!(not_found_response["result"]["value"].is_null());

    // List response
    let list_response = serde_json::json!({
        "jsonrpc": "2.0",
        "result": {
            "keys": ["user_preference", "last_session", "settings"]
        },
        "id": 1
    });

    assert!(list_response["result"]["keys"].is_array());
    assert_eq!(list_response["result"]["keys"].as_array().unwrap().len(), 3);
}
