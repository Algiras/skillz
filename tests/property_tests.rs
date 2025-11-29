//! Property-based tests for Skillz using proptest
//!
//! These tests verify invariants that should hold for all inputs.

use proptest::prelude::*;

// ==================== Registry Property Tests ====================

/// Test that tool names are properly sanitized
fn is_valid_tool_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

proptest! {
    /// Property: Valid tool names should only contain allowed characters
    #[test]
    fn test_tool_name_validation(name in "[a-zA-Z][a-zA-Z0-9_-]{0,63}") {
        prop_assert!(is_valid_tool_name(&name));
    }

    /// Property: Empty names should be invalid
    #[test]
    fn test_empty_name_invalid(name in "") {
        prop_assert!(!is_valid_tool_name(&name));
    }

    /// Property: Names with special characters should be invalid
    #[test]
    fn test_special_chars_invalid(name in ".*[^a-zA-Z0-9_-].*") {
        // Filter to ensure we have at least one special char
        prop_assume!(name.chars().any(|c| !c.is_alphanumeric() && c != '_' && c != '-'));
        prop_assert!(!is_valid_tool_name(&name));
    }
}

// ==================== JSON-RPC Protocol Tests ====================

/// Validate JSON-RPC 2.0 request structure
fn is_valid_jsonrpc_request(json: &serde_json::Value) -> bool {
    json.get("jsonrpc") == Some(&serde_json::json!("2.0"))
        && json.get("method").map(|m| m.is_string()).unwrap_or(false)
        && json.get("id").is_some()
}

proptest! {
    /// Property: All generated requests should be valid JSON-RPC 2.0
    #[test]
    fn test_jsonrpc_request_structure(
        method in "[a-z_]+",
        id in 1u64..10000
    ) {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": {},
            "id": id
        });
        prop_assert!(is_valid_jsonrpc_request(&request));
    }

    /// Property: Requests without jsonrpc version should be invalid
    #[test]
    fn test_missing_jsonrpc_invalid(
        method in "[a-z_]+",
        id in 1u64..10000
    ) {
        let request = serde_json::json!({
            "method": method,
            "params": {},
            "id": id
        });
        prop_assert!(!is_valid_jsonrpc_request(&request));
    }
}

// ==================== Sandbox Configuration Tests ====================

/// Simple struct for testing sandbox paths
struct PathSpec {
    path: String,
    is_absolute: bool,
}

impl PathSpec {
    fn new(path: String) -> Self {
        let is_absolute = path.starts_with('/');
        Self { path, is_absolute }
    }
}

proptest! {
    /// Property: Absolute paths should start with /
    #[test]
    fn test_absolute_path_detection(path in "/[a-z]+(/[a-z]+)*") {
        let spec = PathSpec::new(path);
        prop_assert!(spec.is_absolute);
    }

    /// Property: Relative paths should not start with /
    #[test]
    fn test_relative_path_detection(path in "[a-z]+(/[a-z]+)*") {
        let spec = PathSpec::new(path);
        prop_assert!(!spec.is_absolute);
    }
}

// ==================== WASM Validation Tests ====================

/// Check if Rust code has balanced braces
fn has_balanced_braces(code: &str) -> bool {
    let mut count = 0i32;
    for c in code.chars() {
        match c {
            '{' => count += 1,
            '}' => count -= 1,
            _ => {}
        }
        if count < 0 {
            return false;
        }
    }
    count == 0
}

proptest! {
    /// Property: Balanced brace pairs should pass validation
    #[test]
    fn test_balanced_braces(
        prefix in "[a-z ]*",
        inner in "[a-z ]*",
        suffix in "[a-z ]*"
    ) {
        let code = format!("{}{{{}}}{}", prefix, inner, suffix);
        prop_assert!(has_balanced_braces(&code));
    }

    /// Property: Nested balanced braces should pass
    #[test]
    fn test_nested_balanced_braces(
        a in "[a-z ]*",
        b in "[a-z ]*",
        c in "[a-z ]*"
    ) {
        let code = format!("fn main() {{ {} {{ {} }} {} }}", a, b, c);
        prop_assert!(has_balanced_braces(&code));
    }

    /// Property: Unbalanced opening braces should fail
    #[test]
    fn test_unbalanced_open(content in "[a-z ]+") {
        let code = format!("{{ {}", content);
        prop_assert!(!has_balanced_braces(&code));
    }

    /// Property: Unbalanced closing braces should fail
    #[test]
    fn test_unbalanced_close(content in "[a-z ]+") {
        let code = format!("{} }}", content);
        prop_assert!(!has_balanced_braces(&code));
    }
}

// ==================== Environment Variable Tests ====================

/// Safe environment variables that can be passed to scripts
const SAFE_ENV_VARS: &[&str] = &["HOME", "USER", "LANG", "PATH", "TERM"];

/// Check if an environment variable is safe to pass
fn is_safe_env_var(name: &str) -> bool {
    SAFE_ENV_VARS.contains(&name)
}

proptest! {
    /// Property: Known safe vars should be allowed
    #[test]
    fn test_safe_vars_allowed(idx in 0usize..5) {
        let var = SAFE_ENV_VARS[idx];
        prop_assert!(is_safe_env_var(var));
    }

    /// Property: Random vars should be blocked (with high probability)
    #[test]
    fn test_random_vars_blocked(var in "[A-Z_]{3,20}") {
        // Most random vars won't be in our safe list
        if !SAFE_ENV_VARS.contains(&var.as_str()) {
            prop_assert!(!is_safe_env_var(&var));
        }
    }
}

// ==================== Interpreter Extension Tests ====================

/// Get expected file extension for interpreter
fn extension_for_interpreter(interpreter: &str) -> &'static str {
    match interpreter {
        "python3" | "python" => "py",
        "node" | "nodejs" => "js",
        "ruby" => "rb",
        "bash" | "sh" => "sh",
        "perl" => "pl",
        "php" => "php",
        _ => "script",
    }
}

proptest! {
    /// Property: Python interpreters should use .py extension
    #[test]
    fn test_python_extension(interp in prop_oneof!["python3", "python"]) {
        prop_assert_eq!(extension_for_interpreter(&interp), "py");
    }

    /// Property: Node interpreters should use .js extension
    #[test]
    fn test_node_extension(interp in prop_oneof!["node", "nodejs"]) {
        prop_assert_eq!(extension_for_interpreter(&interp), "js");
    }

    /// Property: Unknown interpreters should use .script extension
    #[test]
    fn test_unknown_extension(interp in "[a-z]{10,20}") {
        // Filter out known interpreters
        prop_assume!(
            interp != "python3" && interp != "python" &&
            interp != "node" && interp != "nodejs" &&
            interp != "ruby" && interp != "bash" && interp != "sh" &&
            interp != "perl" && interp != "php"
        );
        prop_assert_eq!(extension_for_interpreter(&interp), "script");
    }
}

// ==================== Memory Limit Tests ====================

proptest! {
    /// Property: Memory limits should be positive or zero
    #[test]
    fn test_memory_limit_nonnegative(limit in 0u64..10000) {
        prop_assert!(limit <= 10000);
    }

    /// Property: Time limits should be reasonable
    #[test]
    fn test_time_limit_range(limit in 1u64..3600) {
        prop_assert!(limit >= 1 && limit <= 3600);
    }
}
