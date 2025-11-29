//! Tests for the WASM builder module

use std::fs;

mod common;

/// Test that valid Rust code compiles to WASM successfully
#[test]
fn test_compile_simple_code() {
    let code = r#"
fn main() {
    println!("Hello from test WASM!");
}
"#;

    let result = common::compile_test_tool("test_simple", code);
    assert!(
        result.is_ok(),
        "Compilation should succeed: {:?}",
        result.err()
    );

    let wasm_path = result.unwrap();
    assert!(wasm_path.exists(), "WASM file should exist");

    // Clean up
    let _ = fs::remove_file(wasm_path);
}

/// Test that code with syntax errors fails to compile
#[test]
fn test_compile_invalid_code() {
    let code = r#"
fn main() {
    this is not valid rust
}
"#;

    let result = common::compile_test_tool("test_invalid", code);
    assert!(result.is_err(), "Compilation should fail for invalid code");
}

/// Test compilation of code with dependencies (std only)
#[test]
fn test_compile_with_std() {
    let code = r#"
use std::collections::HashMap;

fn main() {
    let mut map = HashMap::new();
    map.insert("key", "value");
    println!("{:?}", map);
}
"#;

    let result = common::compile_test_tool("test_std", code);
    assert!(
        result.is_ok(),
        "Compilation with std should succeed: {:?}",
        result.err()
    );

    if let Ok(path) = result {
        let _ = fs::remove_file(path);
    }
}

/// Test that code without main function fails
#[test]
fn test_compile_no_main() {
    let code = r#"
fn helper() {
    println!("No main!");
}
"#;

    let result = common::compile_test_tool("test_no_main", code);
    assert!(result.is_err(), "Compilation should fail without main");
}

/// Test compilation produces valid WASM binary
#[test]
fn test_wasm_binary_valid() {
    let code = r#"
fn main() {
    println!("WASM binary test");
}
"#;

    let result = common::compile_test_tool("test_binary", code);
    assert!(result.is_ok());

    let wasm_path = result.unwrap();
    let bytes = fs::read(&wasm_path).expect("Should read WASM file");

    // WASM magic number: 0x00 0x61 0x73 0x6D (\0asm)
    assert!(bytes.len() >= 4, "WASM file should have at least 4 bytes");
    assert_eq!(
        &bytes[0..4],
        &[0x00, 0x61, 0x73, 0x6D],
        "Should have WASM magic number"
    );

    let _ = fs::remove_file(wasm_path);
}
