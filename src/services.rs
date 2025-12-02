//! Docker service management for Skillz tools
//!
//! Allows tools to declare service dependencies (databases, caches, etc.)
//! that are managed via Docker containers.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, RwLock};

/// Health check configuration for a service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    /// Command to run inside the container
    pub cmd: String,
    /// Interval between checks (e.g., "2s", "500ms")
    #[serde(default = "default_interval")]
    pub interval: String,
    /// Number of retries before giving up
    #[serde(default = "default_retries")]
    pub retries: u32,
    /// Timeout for each check
    #[serde(default = "default_timeout")]
    pub timeout: String,
}

fn default_interval() -> String {
    "2s".to_string()
}
fn default_retries() -> u32 {
    15
}
fn default_timeout() -> String {
    "5s".to_string()
}

/// A service definition (stored in $TOOLS_DIR/services/)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDefinition {
    /// Unique name for the service
    pub name: String,
    /// Docker image (e.g., "postgres:15", "redis:alpine")
    pub image: String,
    /// Port mappings: ["5432"] for random host port, or ["5432:5432"] for fixed
    #[serde(default)]
    pub ports: Vec<String>,
    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Volume mounts: ["data:/var/lib/data", "/host/path:/container/path"]
    #[serde(default)]
    pub volumes: Vec<String>,
    /// Health check configuration
    #[serde(default)]
    pub healthcheck: Option<HealthCheck>,
    /// Optional description
    #[serde(default)]
    pub description: Option<String>,
    /// Docker network to join
    #[serde(default = "default_network")]
    pub network: String,
}

fn default_network() -> String {
    "skillz_services".to_string()
}

impl ServiceDefinition {
    /// Get the container name for this service
    pub fn container_name(&self) -> String {
        format!("skillz_svc_{}", self.name)
    }

    /// Get the volume name (prefixed for Skillz)
    pub fn volume_name(&self, vol: &str) -> String {
        // If it's a named volume (no / at start), prefix it
        if !vol.starts_with('/') && !vol.contains(':') {
            format!("skillz_{}", vol)
        } else if let Some((name, _)) = vol.split_once(':') {
            if !name.starts_with('/') && !name.contains('/') {
                format!("skillz_{}", vol)
            } else {
                vol.to_string()
            }
        } else {
            vol.to_string()
        }
    }
}

/// Status of a running service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub name: String,
    pub container_id: Option<String>,
    pub status: String, // "running", "stopped", "not_created", "unhealthy"
    pub ports: HashMap<String, String>, // container_port -> host_port
    pub health: Option<String>,
    pub uptime: Option<String>,
}

/// Manages service definitions and Docker containers
#[derive(Clone)]
pub struct ServiceRegistry {
    services_dir: PathBuf,
    definitions: Arc<RwLock<HashMap<String, ServiceDefinition>>>,
}

impl ServiceRegistry {
    pub fn new(tools_dir: &PathBuf) -> Self {
        let services_dir = tools_dir.join("services");
        std::fs::create_dir_all(&services_dir).ok();

        let registry = Self {
            services_dir,
            definitions: Arc::new(RwLock::new(HashMap::new())),
        };

        // Load existing service definitions
        registry.load_definitions();

        // Ensure the skillz network exists
        registry.ensure_network();

        registry
    }

    /// Ensure Docker is available
    pub fn check_docker() -> Result<(), String> {
        let output = Command::new("docker")
            .args(["version", "--format", "{{.Server.Version}}"])
            .output()
            .map_err(|e| format!("Docker not available: {}", e))?;

        if !output.status.success() {
            return Err("Docker daemon is not running".to_string());
        }

        Ok(())
    }

    /// Ensure the skillz_services network exists
    fn ensure_network(&self) {
        let _ = Command::new("docker")
            .args(["network", "create", "skillz_services"])
            .output();
    }

    /// Load service definitions from disk
    fn load_definitions(&self) {
        if let Ok(entries) = std::fs::read_dir(&self.services_dir) {
            let mut defs = self.definitions.write().unwrap();
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "json") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(def) = serde_json::from_str::<ServiceDefinition>(&content) {
                            defs.insert(def.name.clone(), def);
                        }
                    }
                }
            }
        }
    }

    /// Save a service definition to disk
    fn save_definition(&self, def: &ServiceDefinition) -> Result<(), String> {
        let path = self.services_dir.join(format!("{}.json", def.name));
        let content =
            serde_json::to_string_pretty(def).map_err(|e| format!("Failed to serialize: {}", e))?;
        std::fs::write(&path, content).map_err(|e| format!("Failed to write: {}", e))?;
        Ok(())
    }

    /// Define a new service (or update existing)
    pub fn define(&self, def: ServiceDefinition, overwrite: bool) -> Result<String, String> {
        Self::check_docker()?;

        {
            let defs = self.definitions.read().unwrap();
            if defs.contains_key(&def.name) && !overwrite {
                return Err(format!(
                    "Service '{}' already exists. Use overwrite: true to update.",
                    def.name
                ));
            }
        }

        self.save_definition(&def)?;

        let mut defs = self.definitions.write().unwrap();
        let name = def.name.clone();
        defs.insert(name.clone(), def);

        Ok(format!("Service '{}' defined successfully", name))
    }

    /// Get a service definition
    pub fn get(&self, name: &str) -> Option<ServiceDefinition> {
        self.definitions.read().unwrap().get(name).cloned()
    }

    /// List all defined services with their status
    pub fn list(&self) -> Result<Vec<ServiceStatus>, String> {
        Self::check_docker()?;

        let defs = self.definitions.read().unwrap();
        let mut statuses = Vec::new();

        for (name, _def) in defs.iter() {
            statuses.push(self.get_status(name)?);
        }

        Ok(statuses)
    }

    /// Get status of a specific service
    pub fn get_status(&self, name: &str) -> Result<ServiceStatus, String> {
        let def = self
            .get(name)
            .ok_or_else(|| format!("Service '{}' not defined", name))?;

        let container_name = def.container_name();

        // Check if container exists and get its status
        let output = Command::new("docker")
            .args([
                "inspect",
                "--format",
                "{{.State.Status}}|{{.Id}}",
                &container_name,
            ])
            .output()
            .map_err(|e| format!("Docker command failed: {}", e))?;

        if !output.status.success() {
            return Ok(ServiceStatus {
                name: name.to_string(),
                container_id: None,
                status: "not_created".to_string(),
                ports: HashMap::new(),
                health: None,
                uptime: None,
            });
        }

        let info = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = info.trim().split('|').collect();

        let status = parts.first().unwrap_or(&"unknown").to_string();
        let container_id = parts.get(1).map(|s| {
            if s.len() >= 12 {
                s[..12].to_string()
            } else {
                s.to_string()
            }
        });

        // Get port mappings
        let ports = self.get_port_mappings(&container_name)?;

        // Get health status separately (may not exist)
        let health = self.get_health_status(&container_name).ok().flatten();

        // Get uptime
        let uptime = self.get_uptime(&container_name).ok();

        Ok(ServiceStatus {
            name: name.to_string(),
            container_id,
            status,
            ports,
            health,
            uptime,
        })
    }

    /// Get health status for a container (if health check is configured)
    fn get_health_status(&self, container_name: &str) -> Result<Option<String>, String> {
        let output = Command::new("docker")
            .args([
                "inspect",
                "--format",
                "{{if .State.Health}}{{.State.Health.Status}}{{end}}",
                container_name,
            ])
            .output()
            .map_err(|e| format!("Docker command failed: {}", e))?;

        if output.status.success() {
            let health = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if health.is_empty() {
                Ok(None)
            } else {
                Ok(Some(health))
            }
        } else {
            Ok(None)
        }
    }

    /// Get port mappings for a container
    fn get_port_mappings(&self, container_name: &str) -> Result<HashMap<String, String>, String> {
        let output = Command::new("docker")
            .args(["port", container_name])
            .output()
            .map_err(|e| format!("Docker command failed: {}", e))?;

        let mut ports = HashMap::new();

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                // Format: "5432/tcp -> 0.0.0.0:32768"
                if let Some((container_port, host_binding)) = line.split_once(" -> ") {
                    let container_port = container_port.split('/').next().unwrap_or(container_port);
                    let host_port = host_binding.rsplit(':').next().unwrap_or(host_binding);
                    ports.insert(container_port.to_string(), host_port.to_string());
                }
            }
        }

        Ok(ports)
    }

    /// Get container uptime
    fn get_uptime(&self, container_name: &str) -> Result<String, String> {
        let output = Command::new("docker")
            .args([
                "inspect",
                "--format",
                "{{.State.StartedAt}}",
                container_name,
            ])
            .output()
            .map_err(|e| format!("Docker command failed: {}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err("Container not running".to_string())
        }
    }

    /// Start a service
    pub fn start(&self, name: &str) -> Result<ServiceStatus, String> {
        Self::check_docker()?;

        let def = self
            .get(name)
            .ok_or_else(|| format!("Service '{}' not defined", name))?;

        let container_name = def.container_name();

        // Check if container already exists
        let status = self.get_status(name)?;

        if status.status == "running" {
            return Ok(status);
        }

        if status.container_id.is_some() {
            // Container exists but stopped, start it
            let output = Command::new("docker")
                .args(["start", &container_name])
                .output()
                .map_err(|e| format!("Failed to start container: {}", e))?;

            if !output.status.success() {
                return Err(format!(
                    "Failed to start: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
        } else {
            // Need to create the container
            self.create_container(&def)?;
        }

        // Wait for health check if configured
        if def.healthcheck.is_some() {
            self.wait_healthy(name, 30)?;
        } else {
            // Brief wait for container to initialize
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        self.get_status(name)
    }

    /// Create a new container for a service
    fn create_container(&self, def: &ServiceDefinition) -> Result<(), String> {
        let mut args = vec![
            "run".to_string(),
            "-d".to_string(),
            "--name".to_string(),
            def.container_name(),
            "--network".to_string(),
            def.network.clone(),
            "--restart".to_string(),
            "unless-stopped".to_string(),
        ];

        // Add ports
        for port in &def.ports {
            args.push("-p".to_string());
            if port.contains(':') {
                args.push(port.clone());
            } else {
                // Random host port
                args.push(format!(":{}", port));
            }
        }

        // Add environment variables
        for (key, value) in &def.env {
            args.push("-e".to_string());
            args.push(format!("{}={}", key, value));
        }

        // Add volumes
        for vol in &def.volumes {
            args.push("-v".to_string());
            args.push(def.volume_name(vol));
        }

        // Add health check if configured
        if let Some(hc) = &def.healthcheck {
            args.push("--health-cmd".to_string());
            args.push(hc.cmd.clone());
            args.push("--health-interval".to_string());
            args.push(hc.interval.clone());
            args.push("--health-retries".to_string());
            args.push(hc.retries.to_string());
            args.push("--health-timeout".to_string());
            args.push(hc.timeout.clone());
        }

        // Add image
        args.push(def.image.clone());

        let output = Command::new("docker")
            .args(&args)
            .output()
            .map_err(|e| format!("Failed to create container: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "Failed to create container: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(())
    }

    /// Wait for a service to become healthy
    fn wait_healthy(&self, name: &str, timeout_secs: u32) -> Result<(), String> {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(timeout_secs as u64);

        loop {
            if start.elapsed() > timeout {
                return Err(format!(
                    "Service '{}' did not become healthy within {}s",
                    name, timeout_secs
                ));
            }

            let status = self.get_status(name)?;

            match status.health.as_deref() {
                Some("healthy") => return Ok(()),
                Some("unhealthy") => {
                    return Err(format!("Service '{}' is unhealthy", name));
                }
                _ => {
                    // Still starting, wait a bit
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            }
        }
    }

    /// Stop a service (keeps container for restart)
    pub fn stop(&self, name: &str) -> Result<String, String> {
        Self::check_docker()?;

        let def = self
            .get(name)
            .ok_or_else(|| format!("Service '{}' not defined", name))?;

        let output = Command::new("docker")
            .args(["stop", &def.container_name()])
            .output()
            .map_err(|e| format!("Failed to stop: {}", e))?;

        if output.status.success() {
            Ok(format!("Service '{}' stopped", name))
        } else {
            // Container might not exist, which is fine
            Ok(format!("Service '{}' was not running", name))
        }
    }

    /// Remove a service (stops and removes container, optionally volumes)
    pub fn remove(&self, name: &str, remove_volumes: bool) -> Result<String, String> {
        Self::check_docker()?;

        let def = self
            .get(name)
            .ok_or_else(|| format!("Service '{}' not defined", name))?;

        // Stop and remove container
        let _ = Command::new("docker")
            .args(["rm", "-f", &def.container_name()])
            .output();

        // Remove volumes if requested
        if remove_volumes {
            for vol in &def.volumes {
                if let Some((vol_name, _)) = vol.split_once(':') {
                    if !vol_name.starts_with('/') {
                        let prefixed = format!("skillz_{}", vol_name);
                        let _ = Command::new("docker")
                            .args(["volume", "rm", &prefixed])
                            .output();
                    }
                }
            }
        }

        // Remove definition
        {
            let mut defs = self.definitions.write().unwrap();
            defs.remove(name);
        }

        // Remove file
        let path = self.services_dir.join(format!("{}.json", name));
        let _ = std::fs::remove_file(path);

        Ok(format!(
            "Service '{}' removed{}",
            name,
            if remove_volumes {
                " (with volumes)"
            } else {
                ""
            }
        ))
    }

    /// Get logs from a service
    pub fn logs(&self, name: &str, tail: Option<u32>) -> Result<String, String> {
        Self::check_docker()?;

        let def = self
            .get(name)
            .ok_or_else(|| format!("Service '{}' not defined", name))?;

        let mut args = vec!["logs".to_string()];

        if let Some(n) = tail {
            args.push("--tail".to_string());
            args.push(n.to_string());
        }

        args.push(def.container_name());

        let output = Command::new("docker")
            .args(&args)
            .output()
            .map_err(|e| format!("Failed to get logs: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        Ok(format!("{}{}", stdout, stderr))
    }

    /// Prune stopped containers and optionally unused volumes
    pub fn prune(&self, include_volumes: bool) -> Result<String, String> {
        Self::check_docker()?;

        let mut result = String::new();

        // Remove stopped skillz containers
        let output = Command::new("docker")
            .args(["container", "prune", "-f", "--filter", "name=skillz_svc_"])
            .output()
            .map_err(|e| format!("Failed to prune containers: {}", e))?;

        result.push_str(&String::from_utf8_lossy(&output.stdout));

        if include_volumes {
            // Remove unused skillz volumes
            let output = Command::new("docker")
                .args(["volume", "prune", "-f", "--filter", "name=skillz_"])
                .output()
                .map_err(|e| format!("Failed to prune volumes: {}", e))?;

            result.push_str(&String::from_utf8_lossy(&output.stdout));
        }

        Ok(result)
    }

    /// Check if all required services are running
    /// Returns Ok with env vars if all running, Err with helpful message if not
    pub fn check_required_services(
        &self,
        required: &[String],
    ) -> Result<HashMap<String, String>, String> {
        if required.is_empty() {
            return Ok(HashMap::new());
        }

        Self::check_docker()?;

        let mut env_vars = HashMap::new();
        let mut missing = Vec::new();
        let mut stopped = Vec::new();

        for name in required {
            match self.get_status(name) {
                Ok(status) => {
                    if status.status == "running" {
                        // Inject env vars for this service
                        let prefix = name.to_uppercase().replace('-', "_");
                        env_vars.insert(format!("{}_HOST", prefix), "localhost".to_string());

                        // Get the first port as the main port
                        if let Some((container_port, host_port)) = status.ports.iter().next() {
                            env_vars.insert(format!("{}_PORT", prefix), host_port.clone());
                            env_vars.insert(
                                format!("{}_{}_PORT", prefix, container_port),
                                host_port.clone(),
                            );
                        }

                        // Also add container name for inter-service communication
                        if let Some(def) = self.get(name) {
                            env_vars.insert(format!("{}_CONTAINER", prefix), def.container_name());
                        }
                    } else {
                        stopped.push((name.clone(), status.status));
                    }
                }
                Err(_) => {
                    missing.push(name.clone());
                }
            }
        }

        if !missing.is_empty() || !stopped.is_empty() {
            let mut msg = String::from("🐳 Service dependencies not satisfied!\n\n");

            if !missing.is_empty() {
                msg.push_str("❌ Not defined:\n");
                for name in &missing {
                    msg.push_str(&format!("   • {} - run: services(action: \"define\", name: \"{}\", image: \"...\")\n", name, name));
                }
                msg.push('\n');
            }

            if !stopped.is_empty() {
                msg.push_str("⏸️  Not running:\n");
                for (name, status) in &stopped {
                    msg.push_str(&format!(
                        "   • {} ({}) - run: services(action: \"start\", name: \"{}\")\n",
                        name, status, name
                    ));
                }
            }

            msg.push_str("\n💡 Tip: Define services once, then they're available to all tools!");

            return Err(msg);
        }

        Ok(env_vars)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_name() {
        let def = ServiceDefinition {
            name: "postgres".to_string(),
            image: "postgres:15".to_string(),
            ports: vec![],
            env: HashMap::new(),
            volumes: vec![],
            healthcheck: None,
            description: None,
            network: "skillz_services".to_string(),
        };

        assert_eq!(def.container_name(), "skillz_svc_postgres");
    }

    #[test]
    fn test_volume_prefixing() {
        let def = ServiceDefinition {
            name: "test".to_string(),
            image: "test".to_string(),
            ports: vec![],
            env: HashMap::new(),
            volumes: vec![],
            healthcheck: None,
            description: None,
            network: "skillz_services".to_string(),
        };

        // Named volume gets prefixed
        assert_eq!(
            def.volume_name("data:/var/lib/data"),
            "skillz_data:/var/lib/data"
        );

        // Bind mount stays as-is
        assert_eq!(
            def.volume_name("/host/path:/container/path"),
            "/host/path:/container/path"
        );
    }
}
