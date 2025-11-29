use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

pub struct Builder;

impl Builder {
    fn get_cargo_path() -> String {
        // Try to find cargo in common locations
        if let Ok(home) = std::env::var("HOME") {
            let cargo_home = format!("{}/.cargo/bin/cargo", home);
            if std::path::Path::new(&cargo_home).exists() {
                return cargo_home;
            }
        }
        "cargo".to_string()
    }

    pub fn compile_tool(name: &str, code: &str) -> Result<PathBuf> {
        let temp_dir = TempDir::new()?;
        let package_name = name.replace(" ", "_").to_lowercase();
        let project_path = temp_dir.path().join(&package_name);
        let cargo = Self::get_cargo_path();

        // Create cargo project directly as bin
        let output = Command::new(&cargo)
            .arg("new")
            .arg("--bin")
            .arg(&project_path)
            .output()
            .context("Failed to run cargo new")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create cargo project: {}", stderr);
        }

        let src_path = project_path.join("src/main.rs");
        fs::write(&src_path, code).context("Failed to write source code")?;

        // Build with full output capture
        let output = Command::new(&cargo)
            .current_dir(&project_path)
            .arg("build")
            .arg("--release")
            .arg("--target")
            .arg("wasm32-wasip1")
            .output()
            .context("Failed to run cargo build")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            anyhow::bail!("Cargo build failed:\n{}\n{}", stderr, stdout);
        }

        let wasm_path = project_path
            .join("target/wasm32-wasip1/release")
            .join(format!("{}.wasm", package_name));

        if !wasm_path.exists() {
            anyhow::bail!("WASM artifact not found at {:?}", wasm_path);
        }

        // Return the path to the temp file - wait, temp dir will be deleted.
        // We need to copy it out.
        // The caller should handle copying to permanent storage.
        // For now, we return the path inside the temp dir, but we must ensure
        // the temp dir persists or we copy it here.
        // Let's copy to a temp file that persists.

        let output_path = std::env::temp_dir().join(format!("{}.wasm", package_name));
        fs::copy(&wasm_path, &output_path)?;

        Ok(output_path)
    }
}
