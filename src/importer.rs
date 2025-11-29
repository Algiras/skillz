//! Tool importer - Import tools from git repositories and GitHub gists

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::registry::{ToolManifest, ToolRegistry, ToolType};

/// Source type for importing tools
#[derive(Debug, Clone)]
pub enum ImportSource {
    /// Git repository URL
    Git { url: String, branch: Option<String> },
    /// GitHub Gist ID or URL
    Gist { id: String },
    /// Direct URL to a zip/tar file
    Url { url: String },
}

impl ImportSource {
    /// Parse a source string into ImportSource
    pub fn parse(source: &str) -> Result<Self> {
        let source = source.trim();

        // Gist format: "gist:ID" or "https://gist.github.com/user/ID"
        if source.starts_with("gist:") {
            let id = source.strip_prefix("gist:").unwrap().trim();
            return Ok(Self::Gist { id: id.to_string() });
        }

        if source.contains("gist.github.com") {
            // Extract gist ID from URL
            let id = source
                .split('/')
                .last()
                .unwrap_or("")
                .split('.')
                .next()
                .unwrap_or("");
            if !id.is_empty() {
                return Ok(Self::Gist { id: id.to_string() });
            }
        }

        // Git URL formats
        if source.ends_with(".git")
            || source.starts_with("git@")
            || source.starts_with("https://github.com")
            || source.starts_with("https://gitlab.com")
            || source.starts_with("https://bitbucket.org")
        {
            // Check for branch specification: url#branch
            if let Some((url, branch)) = source.split_once('#') {
                return Ok(Self::Git {
                    url: url.to_string(),
                    branch: Some(branch.to_string()),
                });
            }
            return Ok(Self::Git {
                url: source.to_string(),
                branch: None,
            });
        }

        // Direct URL (zip/tar)
        if source.starts_with("http://") || source.starts_with("https://") {
            return Ok(Self::Url {
                url: source.to_string(),
            });
        }

        anyhow::bail!(
            "Unknown source format: {}. Expected git URL, gist:ID, or https:// URL",
            source
        )
    }
}

/// Result of an import operation
#[derive(Debug)]
pub struct ImportResult {
    pub tool_name: String,
    pub tool_type: ToolType,
    pub source: String,
    pub message: String,
}

/// Tool importer
pub struct Importer {
    storage_dir: PathBuf,
}

impl Importer {
    pub fn new(storage_dir: PathBuf) -> Self {
        Self { storage_dir }
    }

    /// Import a tool from a source
    pub fn import(
        &self,
        source: &str,
        registry: &ToolRegistry,
        overwrite: bool,
    ) -> Result<ImportResult> {
        let import_source = ImportSource::parse(source)?;

        match import_source {
            ImportSource::Git { url, branch } => self.import_from_git(&url, branch.as_deref(), registry, overwrite),
            ImportSource::Gist { id } => self.import_from_gist(&id, registry, overwrite),
            ImportSource::Url { url } => self.import_from_url(&url, registry, overwrite),
        }
    }

    /// Import from a git repository
    fn import_from_git(
        &self,
        url: &str,
        branch: Option<&str>,
        registry: &ToolRegistry,
        overwrite: bool,
    ) -> Result<ImportResult> {
        // Create temp directory for clone
        let temp_dir = tempfile::tempdir()?;
        let clone_path = temp_dir.path();

        // Clone the repository
        let mut cmd = Command::new("git");
        cmd.arg("clone").arg("--depth").arg("1");

        if let Some(b) = branch {
            cmd.arg("--branch").arg(b);
        }

        cmd.arg(url).arg(clone_path);

        let output = cmd.output().context("Failed to run git clone")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Git clone failed: {}", stderr);
        }

        // Look for manifest.json
        let manifest_path = clone_path.join("manifest.json");
        if !manifest_path.exists() {
            anyhow::bail!(
                "No manifest.json found in repository. Expected at root level."
            );
        }

        // Parse manifest
        let manifest_content = fs::read_to_string(&manifest_path)?;
        let manifest: ToolManifest = serde_json::from_str(&manifest_content)
            .context("Failed to parse manifest.json")?;

        // Check if tool exists
        if registry.get_tool(&manifest.name).is_some() && !overwrite {
            anyhow::bail!(
                "Tool '{}' already exists. Use overwrite=true to replace it.",
                manifest.name
            );
        }

        // Copy tool to storage
        let tool_dir = self.storage_dir.join(&manifest.name);
        if tool_dir.exists() {
            fs::remove_dir_all(&tool_dir)?;
        }

        // Copy all files from clone to tool directory
        copy_dir_contents(clone_path, &tool_dir)?;

        // Remove .git directory
        let git_dir = tool_dir.join(".git");
        if git_dir.exists() {
            fs::remove_dir_all(&git_dir)?;
        }

        // Register the tool by reloading
        let tool_type = manifest.tool_type.clone();
        let tool_name = manifest.name.clone();

        // The registry will pick it up on next list_tools or we can manually trigger
        // For now, just return success - the tool will be available after reload

        Ok(ImportResult {
            tool_name,
            tool_type,
            source: url.to_string(),
            message: format!("Successfully imported from git. Tool directory: {}", tool_dir.display()),
        })
    }

    /// Import from a GitHub Gist
    fn import_from_gist(
        &self,
        gist_id: &str,
        registry: &ToolRegistry,
        overwrite: bool,
    ) -> Result<ImportResult> {
        // Fetch gist metadata from GitHub API
        let api_url = format!("https://api.github.com/gists/{}", gist_id);

        let output = Command::new("curl")
            .arg("-s")
            .arg("-H")
            .arg("Accept: application/vnd.github.v3+json")
            .arg(&api_url)
            .output()
            .context("Failed to fetch gist")?;

        if !output.status.success() {
            anyhow::bail!("Failed to fetch gist from GitHub API");
        }

        let gist_json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .context("Failed to parse gist response")?;

        // Check for errors
        if let Some(message) = gist_json.get("message") {
            anyhow::bail!("GitHub API error: {}", message);
        }

        let files = gist_json
            .get("files")
            .and_then(|f| f.as_object())
            .context("No files in gist")?;

        // Require manifest.json for proper tool configuration
        let manifest_file = files.get("manifest.json")
            .ok_or_else(|| anyhow::anyhow!(
                "Gist must contain a manifest.json file.\n\n\
                Create a manifest.json with at minimum:\n\
                {{\n  \
                  \"name\": \"tool_name\",\n  \
                  \"description\": \"What the tool does\",\n  \
                  \"tool_type\": \"script\",\n  \
                  \"interpreter\": \"python3\"\n\
                }}"
            ))?;

        // Download manifest
        let manifest_content = manifest_file
            .get("content")
            .and_then(|c| c.as_str())
            .context("Could not get manifest content")?;

        let manifest: ToolManifest = serde_json::from_str(manifest_content)
            .context("Failed to parse manifest.json from gist")?;

        // Check if tool exists
        if registry.get_tool(&manifest.name).is_some() && !overwrite {
            anyhow::bail!(
                "Tool '{}' already exists. Use overwrite=true to replace it.",
                manifest.name
            );
        }

        // Create tool directory
        let tool_dir = self.storage_dir.join(&manifest.name);
        if tool_dir.exists() {
            fs::remove_dir_all(&tool_dir)?;
        }
        fs::create_dir_all(&tool_dir)?;

        // Download all files from gist
        for (filename, file_info) in files {
            if let Some(content) = file_info.get("content").and_then(|c| c.as_str()) {
                let file_path = tool_dir.join(filename);
                fs::write(&file_path, content)?;

                // Make scripts executable
                #[cfg(unix)]
                if filename.ends_with(".py")
                    || filename.ends_with(".sh")
                    || filename.ends_with(".rb")
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mut perms = fs::metadata(&file_path)?.permissions();
                    perms.set_mode(0o755);
                    fs::set_permissions(&file_path, perms)?;
                }
            }
        }

        let tool_type = manifest.tool_type.clone();
        let tool_name = manifest.name.clone();

        Ok(ImportResult {
            tool_name,
            tool_type,
            source: format!("gist:{}", gist_id),
            message: format!(
                "Successfully imported from gist. Tool directory: {}",
                tool_dir.display()
            ),
        })
    }

    /// Import from a URL (zip/tar)
    fn import_from_url(
        &self,
        url: &str,
        _registry: &ToolRegistry,
        _overwrite: bool,
    ) -> Result<ImportResult> {
        // TODO: Implement URL import (download and extract zip/tar)
        anyhow::bail!(
            "URL import not yet implemented. Use git: or gist: sources for now. URL: {}",
            url
        )
    }
}

/// Copy directory contents recursively
fn copy_dir_contents(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_contents(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_git_url() {
        let source = ImportSource::parse("https://github.com/user/repo").unwrap();
        assert!(matches!(source, ImportSource::Git { url, branch: None } if url == "https://github.com/user/repo"));
    }

    #[test]
    fn test_parse_git_url_with_branch() {
        let source = ImportSource::parse("https://github.com/user/repo#main").unwrap();
        assert!(matches!(source, ImportSource::Git { url, branch: Some(b) } if url == "https://github.com/user/repo" && b == "main"));
    }

    #[test]
    fn test_parse_gist_short() {
        let source = ImportSource::parse("gist:abc123").unwrap();
        assert!(matches!(source, ImportSource::Gist { id } if id == "abc123"));
    }

    #[test]
    fn test_parse_gist_url() {
        let source = ImportSource::parse("https://gist.github.com/user/abc123").unwrap();
        assert!(matches!(source, ImportSource::Gist { id } if id == "abc123"));
    }
}

