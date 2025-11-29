use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// A dependency for WASM tools
#[derive(Debug, Clone)]
pub struct WasmDependency {
    pub name: String,
    pub version: String,
    /// Optional features to enable
    pub features: Vec<String>,
}

impl WasmDependency {
    #[allow(dead_code)]
    pub fn new(name: &str, version: &str) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
            features: vec![],
        }
    }

    #[allow(dead_code)]
    pub fn with_features(mut self, features: Vec<String>) -> Self {
        self.features = features;
        self
    }

    /// Parse from string format: "name@version" or "name@version[feat1,feat2]"
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }

        // Check for features: name@version[feat1,feat2]
        let (main_part, features) = if let Some(bracket_start) = s.find('[') {
            if let Some(bracket_end) = s.find(']') {
                let features_str = &s[bracket_start + 1..bracket_end];
                let features: Vec<String> = features_str
                    .split(',')
                    .map(|f| f.trim().to_string())
                    .filter(|f| !f.is_empty())
                    .collect();
                (&s[..bracket_start], features)
            } else {
                (s, vec![])
            }
        } else {
            (s, vec![])
        };

        // Parse name@version or just name
        if let Some(at_pos) = main_part.find('@') {
            let name = main_part[..at_pos].trim();
            let version = main_part[at_pos + 1..].trim();
            Some(Self {
                name: name.to_string(),
                version: version.to_string(),
                features,
            })
        } else {
            // Default to latest version
            Some(Self {
                name: main_part.to_string(),
                version: "*".to_string(),
                features,
            })
        }
    }

    /// Convert to Cargo.toml dependency line
    pub fn to_toml_line(&self) -> String {
        if self.features.is_empty() {
            format!("{} = \"{}\"", self.name, self.version)
        } else {
            format!(
                "{} = {{ version = \"{}\", features = [{}] }}",
                self.name,
                self.version,
                self.features
                    .iter()
                    .map(|f| format!("\"{}\"", f))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    }
}

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

    /// Compile a WASM tool with optional dependencies
    #[allow(dead_code)]
    pub fn compile_tool(name: &str, code: &str) -> Result<PathBuf> {
        Self::compile_tool_with_deps(name, code, &[])
    }

    /// Compile a WASM tool with dependencies
    pub fn compile_tool_with_deps(
        name: &str,
        code: &str,
        dependencies: &[WasmDependency],
    ) -> Result<PathBuf> {
        let temp_dir = TempDir::new()?;
        let package_name = name.replace([' ', '-'], "_").to_lowercase();
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

        // Write source code
        let src_path = project_path.join("src/main.rs");
        fs::write(&src_path, code).context("Failed to write source code")?;

        // If there are dependencies, update Cargo.toml
        if !dependencies.is_empty() {
            let cargo_toml_path = project_path.join("Cargo.toml");
            let mut cargo_toml = fs::read_to_string(&cargo_toml_path)?;

            // Add dependencies section if not present
            if !cargo_toml.contains("[dependencies]") {
                cargo_toml.push_str("\n[dependencies]\n");
            }

            // Add each dependency
            for dep in dependencies {
                cargo_toml.push_str(&dep.to_toml_line());
                cargo_toml.push('\n');
            }

            fs::write(&cargo_toml_path, cargo_toml)?;
        }

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

        // Copy to temp file that persists
        let output_path = std::env::temp_dir().join(format!("{}.wasm", package_name));
        fs::copy(&wasm_path, &output_path)?;

        Ok(output_path)
    }

    /// Parse dependency strings into WasmDependency objects
    /// Format: "name@version" or "name@version[feat1,feat2]" or just "name"
    pub fn parse_dependencies(deps: &[String]) -> Vec<WasmDependency> {
        deps.iter()
            .filter_map(|s| WasmDependency::parse(s))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dependency_name_only() {
        let dep = WasmDependency::parse("serde").unwrap();
        assert_eq!(dep.name, "serde");
        assert_eq!(dep.version, "*");
        assert!(dep.features.is_empty());
    }

    #[test]
    fn test_parse_dependency_with_version() {
        let dep = WasmDependency::parse("serde@1.0").unwrap();
        assert_eq!(dep.name, "serde");
        assert_eq!(dep.version, "1.0");
        assert!(dep.features.is_empty());
    }

    #[test]
    fn test_parse_dependency_with_features() {
        let dep = WasmDependency::parse("serde@1.0[derive,json]").unwrap();
        assert_eq!(dep.name, "serde");
        assert_eq!(dep.version, "1.0");
        assert_eq!(dep.features, vec!["derive", "json"]);
    }

    #[test]
    fn test_to_toml_line_simple() {
        let dep = WasmDependency::new("serde", "1.0");
        assert_eq!(dep.to_toml_line(), "serde = \"1.0\"");
    }

    #[test]
    fn test_to_toml_line_with_features() {
        let dep = WasmDependency::new("serde", "1.0").with_features(vec!["derive".to_string()]);
        assert_eq!(
            dep.to_toml_line(),
            "serde = { version = \"1.0\", features = [\"derive\"] }"
        );
    }
}
