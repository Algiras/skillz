//! Common test utilities

use anyhow::Result;
use std::path::PathBuf;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Compile Rust code to WASM for testing
pub fn compile_test_tool(name: &str, code: &str) -> Result<PathBuf> {
    let temp_dir = TempDir::new()?;
    let package_name = name.replace(' ', "_").to_lowercase();
    let project_path = temp_dir.path().join(&package_name);
    
    // Find cargo
    let cargo = get_cargo_path();

    // Create cargo project
    let output = Command::new(&cargo)
        .arg("new")
        .arg("--bin")
        .arg(&project_path)
        .output()?;

    if !output.status.success() {
        anyhow::bail!("Failed to create cargo project: {}", String::from_utf8_lossy(&output.stderr));
    }

    // Write source
    let src_path = project_path.join("src/main.rs");
    fs::write(&src_path, code)?;

    // Build
    let output = Command::new(&cargo)
        .current_dir(&project_path)
        .arg("build")
        .arg("--release")
        .arg("--target")
        .arg("wasm32-wasip1")
        .output()?;

    if !output.status.success() {
        anyhow::bail!("Cargo build failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let wasm_path = project_path
        .join("target/wasm32-wasip1/release")
        .join(format!("{}.wasm", package_name));

    if !wasm_path.exists() {
        anyhow::bail!("WASM file not found");
    }

    // Copy to persistent location
    let output_path = std::env::temp_dir().join(format!("skillz_test_{}.wasm", package_name));
    fs::copy(&wasm_path, &output_path)?;
    
    Ok(output_path)
}

/// Get cargo path
fn get_cargo_path() -> String {
    if let Ok(home) = std::env::var("HOME") {
        let cargo_home = format!("{}/.cargo/bin/cargo", home);
        if std::path::Path::new(&cargo_home).exists() {
            return cargo_home;
        }
    }
    "cargo".to_string()
}

/// Create a temporary tools directory for testing
pub fn create_test_tools_dir() -> TempDir {
    TempDir::new().expect("Failed to create temp dir")
}

