//! Memory storage for AI tools using rusqlite (cross-platform)
//!
//! Provides persistent memory for tools to store state between calls.
//! Uses rusqlite with bundled SQLite for Windows/macOS/Linux support.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Current schema version - increment when making breaking changes
#[allow(dead_code)]
const SCHEMA_VERSION: i32 = 1;

/// Memory store for tool state persistence
#[derive(Clone)]
pub struct Memory {
    conn: Arc<Mutex<Connection>>,
}

impl Memory {
    /// Create a new memory store
    pub async fn new(tools_dir: &Path) -> Result<Self> {
        let db_path = tools_dir.join(".memory.db");

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&db_path)
            .with_context(|| format!("Failed to open memory database at {:?}", db_path))?;

        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        // Run migrations
        store.migrate().await?;

        Ok(store)
    }

    /// Run database migrations
    async fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().await;

        // Create migrations table if not exists
        conn.execute(
            "CREATE TABLE IF NOT EXISTS _migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            [],
        )?;

        // Get current version
        let current_version: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM _migrations",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Apply migrations
        if current_version < 1 {
            Self::migrate_v1(&conn)?;
        }

        Ok(())
    }

    /// Migration v1: Initial schema
    fn migrate_v1(conn: &Connection) -> Result<()> {
        eprintln!("Running memory migration v1...");

        // Main memory table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS memories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tool TEXT NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(tool, key)
            )",
            [],
        )?;

        // Indexes for fast lookups
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_tool ON memories(tool)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_tool_key ON memories(tool, key)",
            [],
        )?;

        // Record migration
        conn.execute("INSERT INTO _migrations (version) VALUES (1)", [])?;

        eprintln!("Memory migration v1 complete");
        Ok(())
    }

    // ==================== Key-Value Operations ====================

    /// Get a value by key
    pub async fn get(&self, tool: &str, key: &str) -> Result<Option<Value>> {
        let conn = self.conn.lock().await;

        let result: Result<String, _> = conn.query_row(
            "SELECT value FROM memories WHERE tool = ? AND key = ?",
            params![tool, key],
            |row| row.get(0),
        );

        match result {
            Ok(json_str) => {
                let value: Value = serde_json::from_str(&json_str)?;
                Ok(Some(value))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Set a value
    pub async fn set(&self, tool: &str, key: &str, value: Value) -> Result<()> {
        let conn = self.conn.lock().await;
        let json_str = serde_json::to_string(&value)?;

        conn.execute(
            "INSERT INTO memories (tool, key, value, updated_at)
             VALUES (?, ?, ?, datetime('now'))
             ON CONFLICT(tool, key) DO UPDATE SET
                value = excluded.value,
                updated_at = datetime('now')",
            params![tool, key, json_str],
        )?;

        Ok(())
    }

    /// List all keys for a tool
    pub async fn list_keys(&self, tool: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().await;

        let mut stmt = conn.prepare("SELECT key FROM memories WHERE tool = ? ORDER BY key")?;
        let keys: Vec<String> = stmt
            .query_map(params![tool], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(keys)
    }

    /// Get all entries for a tool
    #[allow(dead_code)]
    pub async fn get_all(&self, tool: &str) -> Result<Vec<(String, Value)>> {
        let conn = self.conn.lock().await;

        let mut stmt =
            conn.prepare("SELECT key, value FROM memories WHERE tool = ? ORDER BY key")?;
        let entries: Vec<(String, Value)> = stmt
            .query_map(params![tool], |row| {
                let key: String = row.get(0)?;
                let value_str: String = row.get(1)?;
                Ok((key, value_str))
            })?
            .filter_map(|r| r.ok())
            .filter_map(|(k, v)| serde_json::from_str(&v).ok().map(|val| (k, val)))
            .collect();

        Ok(entries)
    }

    /// Delete a key
    pub async fn delete(&self, tool: &str, key: &str) -> Result<bool> {
        let conn = self.conn.lock().await;
        let rows = conn.execute(
            "DELETE FROM memories WHERE tool = ? AND key = ?",
            params![tool, key],
        )?;
        Ok(rows > 0)
    }

    /// Clear all memory for a tool
    pub async fn clear(&self, tool: &str) -> Result<u64> {
        let conn = self.conn.lock().await;
        let rows = conn.execute("DELETE FROM memories WHERE tool = ?", params![tool])?;
        Ok(rows as u64)
    }

    /// Clear all memory (all tools)
    #[allow(dead_code)]
    pub async fn clear_all(&self) -> Result<u64> {
        let conn = self.conn.lock().await;
        let rows = conn.execute("DELETE FROM memories", [])?;
        Ok(rows as u64)
    }

    /// Get statistics about memory usage
    pub async fn stats(&self) -> Result<MemoryStats> {
        let conn = self.conn.lock().await;

        let total_entries: i64 = conn
            .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
            .unwrap_or(0);

        let total_tools: i64 = conn
            .query_row("SELECT COUNT(DISTINCT tool) FROM memories", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        // Get per-tool counts
        let mut stmt = conn.prepare(
            "SELECT tool, COUNT(*) as cnt FROM memories GROUP BY tool ORDER BY cnt DESC LIMIT 10",
        )?;
        let tools_by_count: Vec<(String, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();

        // Get total size
        let total_size: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(LENGTH(value)), 0) FROM memories",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        Ok(MemoryStats {
            total_entries: total_entries as u64,
            total_tools: total_tools as u64,
            total_size_bytes: total_size as u64,
            tools_by_count,
        })
    }
}

/// Memory statistics
#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub total_entries: u64,
    pub total_tools: u64,
    pub total_size_bytes: u64,
    pub tools_by_count: Vec<(String, i64)>,
}

impl std::fmt::Display for MemoryStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "## 📊 Memory Statistics\n")?;
        writeln!(f, "- **Total entries:** {}", self.total_entries)?;
        writeln!(f, "- **Total tools:** {}", self.total_tools)?;
        writeln!(
            f,
            "- **Total size:** {} bytes ({:.2} KB)",
            self.total_size_bytes,
            self.total_size_bytes as f64 / 1024.0
        )?;

        if !self.tools_by_count.is_empty() {
            writeln!(f, "\n### Top Tools by Entry Count\n")?;
            for (tool, count) in &self.tools_by_count {
                writeln!(f, "- **{}**: {} entries", tool, count)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_memory_basic_operations() {
        let temp_dir = TempDir::new().unwrap();
        let memory = Memory::new(temp_dir.path()).await.unwrap();

        // Test set and get
        memory
            .set("test_tool", "key1", serde_json::json!({"value": 42}))
            .await
            .unwrap();

        let result = memory.get("test_tool", "key1").await.unwrap();
        assert_eq!(result, Some(serde_json::json!({"value": 42})));

        // Test non-existent key
        let result = memory.get("test_tool", "nonexistent").await.unwrap();
        assert_eq!(result, None);

        // Test delete
        let deleted = memory.delete("test_tool", "key1").await.unwrap();
        assert!(deleted);

        let result = memory.get("test_tool", "key1").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_memory_list_keys() {
        let temp_dir = TempDir::new().unwrap();
        let memory = Memory::new(temp_dir.path()).await.unwrap();

        memory
            .set("tool1", "alpha", serde_json::json!("a"))
            .await
            .unwrap();
        memory
            .set("tool1", "beta", serde_json::json!("b"))
            .await
            .unwrap();
        memory
            .set("tool2", "gamma", serde_json::json!("c"))
            .await
            .unwrap();

        let keys = memory.list_keys("tool1").await.unwrap();
        assert_eq!(keys, vec!["alpha", "beta"]);
    }

    #[tokio::test]
    async fn test_memory_stats() {
        let temp_dir = TempDir::new().unwrap();
        let memory = Memory::new(temp_dir.path()).await.unwrap();

        memory
            .set("tool1", "key1", serde_json::json!("value1"))
            .await
            .unwrap();
        memory
            .set("tool1", "key2", serde_json::json!("value2"))
            .await
            .unwrap();
        memory
            .set("tool2", "key1", serde_json::json!("value3"))
            .await
            .unwrap();

        let stats = memory.stats().await.unwrap();
        assert_eq!(stats.total_entries, 3);
        assert_eq!(stats.total_tools, 2);
    }

    #[tokio::test]
    async fn test_memory_clear() {
        let temp_dir = TempDir::new().unwrap();
        let memory = Memory::new(temp_dir.path()).await.unwrap();

        memory
            .set("tool1", "key1", serde_json::json!("a"))
            .await
            .unwrap();
        memory
            .set("tool1", "key2", serde_json::json!("b"))
            .await
            .unwrap();

        let cleared = memory.clear("tool1").await.unwrap();
        assert_eq!(cleared, 2);

        let keys = memory.list_keys("tool1").await.unwrap();
        assert!(keys.is_empty());
    }
}
