//! Memory storage for AI tools using libSQL
//!
//! Provides persistent memory for tools to store state between calls.
//! Future: Add vector embeddings for semantic search.

use anyhow::{Context, Result};
use libsql::{Builder, Connection};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Current schema version - increment when making breaking changes
const SCHEMA_VERSION: i32 = 2;

/// Memory store for tool state persistence
#[derive(Clone)]
pub struct Memory {
    conn: Arc<RwLock<Connection>>,
}

impl Memory {
    /// Create a new memory store
    pub async fn new(tools_dir: &Path) -> Result<Self> {
        let db_path = tools_dir.join(".memory.db");

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let db = Builder::new_local(&db_path)
            .build()
            .await
            .with_context(|| format!("Failed to open memory database at {:?}", db_path))?;

        let conn = db.connect()?;

        let store = Self {
            conn: Arc::new(RwLock::new(conn)),
        };

        // Run migrations
        store.migrate().await?;

        Ok(store)
    }

    /// Run database migrations
    async fn migrate(&self) -> Result<()> {
        let conn = self.conn.write().await;

        // Create migrations table if not exists
        conn.execute(
            "CREATE TABLE IF NOT EXISTS _migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            (),
        )
        .await?;

        // Get current version
        let mut rows = conn
            .query("SELECT COALESCE(MAX(version), 0) FROM _migrations", ())
            .await?;

        let current_version: i32 = if let Some(row) = rows.next().await? {
            row.get::<i32>(0)?
        } else {
            0
        };

        // Apply migrations
        if current_version < 1 {
            self.migrate_v1(&conn).await?;
        }

        if current_version < 2 {
            self.migrate_v2(&conn).await?;
        }

        Ok(())
    }

    /// Migration v1: Initial schema
    async fn migrate_v1(&self, conn: &Connection) -> Result<()> {
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
            (),
        )
        .await?;

        // Indexes for fast lookups
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_tool ON memories(tool)",
            (),
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_tool_key ON memories(tool, key)",
            (),
        )
        .await?;

        // Record migration
        conn.execute("INSERT INTO _migrations (version) VALUES (1)", ())
            .await?;

        eprintln!("Memory migration v1 complete");
        Ok(())
    }

    /// Migration v2: Add TTL/expiration support
    async fn migrate_v2(&self, conn: &Connection) -> Result<()> {
        eprintln!("Running memory migration v2 (TTL support)...");

        // Add expires_at column (NULL = never expires)
        conn.execute(
            "ALTER TABLE memories ADD COLUMN expires_at TEXT DEFAULT NULL",
            (),
        )
        .await?;

        // Index for efficient cleanup of expired entries
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_expires ON memories(expires_at) WHERE expires_at IS NOT NULL",
            (),
        )
        .await?;

        // Record migration
        conn.execute("INSERT INTO _migrations (version) VALUES (2)", ())
            .await?;

        eprintln!("Memory migration v2 complete");
        Ok(())
    }

    // ==================== Key-Value Operations ====================

    /// Get a value by key (returns None if expired)
    pub async fn get(&self, tool: &str, key: &str) -> Result<Option<Value>> {
        let conn = self.conn.read().await;
        let mut rows = conn
            .query(
                "SELECT value FROM memories 
                 WHERE tool = ? AND key = ? 
                 AND (expires_at IS NULL OR expires_at > datetime('now'))",
                [tool, key],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            let json_str: String = row.get(0)?;
            let value: Value = serde_json::from_str(&json_str)?;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    /// Set a value (without TTL - never expires)
    pub async fn set(&self, tool: &str, key: &str, value: Value) -> Result<()> {
        self.set_with_ttl(tool, key, value, None).await
    }

    /// Set a value with optional TTL in seconds (None = never expires)
    pub async fn set_with_ttl(
        &self,
        tool: &str,
        key: &str,
        value: Value,
        ttl_secs: Option<u64>,
    ) -> Result<()> {
        let conn = self.conn.write().await;
        let json_str = serde_json::to_string(&value)?;

        // Calculate expiration time if TTL provided
        let expires_at = ttl_secs
            .filter(|&t| t > 0)
            .map(|t| format!("+{} seconds", t));

        match expires_at {
            Some(offset) => {
                conn.execute(
                    &format!(
                        "INSERT INTO memories (tool, key, value, updated_at, expires_at)
                         VALUES (?1, ?2, ?3, datetime('now'), datetime('now', '{}'))
                         ON CONFLICT(tool, key) DO UPDATE SET
                            value = excluded.value,
                            updated_at = datetime('now'),
                            expires_at = datetime('now', '{}')",
                        offset, offset
                    ),
                    [tool, key, &json_str],
                )
                .await?;
            }
            None => {
                conn.execute(
                    "INSERT INTO memories (tool, key, value, updated_at, expires_at)
                     VALUES (?, ?, ?, datetime('now'), NULL)
                     ON CONFLICT(tool, key) DO UPDATE SET
                        value = excluded.value,
                        updated_at = datetime('now'),
                        expires_at = NULL",
                    [tool, key, &json_str],
                )
                .await?;
            }
        }

        Ok(())
    }

    /// List all keys for a tool (excludes expired)
    pub async fn list_keys(&self, tool: &str) -> Result<Vec<String>> {
        let conn = self.conn.read().await;
        let mut rows = conn
            .query(
                "SELECT key FROM memories 
                 WHERE tool = ? 
                 AND (expires_at IS NULL OR expires_at > datetime('now'))
                 ORDER BY key",
                [tool],
            )
            .await?;

        let mut keys = Vec::new();
        while let Some(row) = rows.next().await? {
            keys.push(row.get::<String>(0)?);
        }
        Ok(keys)
    }

    /// Get all entries for a tool (excludes expired)
    #[allow(dead_code)]
    pub async fn get_all(&self, tool: &str) -> Result<Vec<(String, Value)>> {
        let conn = self.conn.read().await;
        let mut rows = conn
            .query(
                "SELECT key, value FROM memories 
                 WHERE tool = ? 
                 AND (expires_at IS NULL OR expires_at > datetime('now'))
                 ORDER BY key",
                [tool],
            )
            .await?;

        let mut entries = Vec::new();
        while let Some(row) = rows.next().await? {
            let key: String = row.get(0)?;
            let value_str: String = row.get(1)?;
            if let Ok(value) = serde_json::from_str(&value_str) {
                entries.push((key, value));
            }
        }
        Ok(entries)
    }

    /// Clean up expired entries (garbage collection)
    #[allow(dead_code)]
    pub async fn cleanup_expired(&self) -> Result<u64> {
        let conn = self.conn.write().await;
        let rows = conn
            .execute(
                "DELETE FROM memories WHERE expires_at IS NOT NULL AND expires_at <= datetime('now')",
                (),
            )
            .await?;
        Ok(rows)
    }

    /// Delete a key
    pub async fn delete(&self, tool: &str, key: &str) -> Result<bool> {
        let conn = self.conn.write().await;
        let rows = conn
            .execute(
                "DELETE FROM memories WHERE tool = ? AND key = ?",
                [tool, key],
            )
            .await?;
        Ok(rows > 0)
    }

    /// Clear all memory for a tool
    pub async fn clear(&self, tool: &str) -> Result<u64> {
        let conn = self.conn.write().await;
        let rows = conn
            .execute("DELETE FROM memories WHERE tool = ?", [tool])
            .await?;
        Ok(rows)
    }

    /// Clear all memory (all tools)
    #[allow(dead_code)]
    pub async fn clear_all(&self) -> Result<u64> {
        let conn = self.conn.write().await;
        let rows = conn.execute("DELETE FROM memories", ()).await?;
        Ok(rows)
    }

    // ==================== Stats ====================

    /// Get memory statistics
    pub async fn stats(&self) -> Result<MemoryStats> {
        let conn = self.conn.read().await;

        let mut rows = conn.query("SELECT COUNT(*) FROM memories", ()).await?;
        let total_entries: i64 = rows
            .next()
            .await?
            .map(|r| r.get(0))
            .transpose()?
            .unwrap_or(0);

        let mut rows = conn
            .query("SELECT COUNT(DISTINCT tool) FROM memories", ())
            .await?;
        let total_tools: i64 = rows
            .next()
            .await?
            .map(|r| r.get(0))
            .transpose()?
            .unwrap_or(0);

        Ok(MemoryStats {
            total_entries: total_entries as u64,
            total_tools: total_tools as u64,
            schema_version: SCHEMA_VERSION,
        })
    }
}

/// Memory statistics
#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub total_entries: u64,
    pub total_tools: u64,
    pub schema_version: i32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_memory() -> (Memory, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let memory = Memory::new(temp_dir.path()).await.unwrap();
        (memory, temp_dir)
    }

    #[tokio::test]
    async fn test_set_and_get() {
        let (memory, _dir) = create_test_memory().await;

        memory
            .set("test_tool", "counter", serde_json::json!(42))
            .await
            .unwrap();
        let value = memory.get("test_tool", "counter").await.unwrap();

        assert_eq!(value, Some(serde_json::json!(42)));
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let (memory, _dir) = create_test_memory().await;

        let value = memory.get("test_tool", "nonexistent").await.unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_update() {
        let (memory, _dir) = create_test_memory().await;

        memory
            .set("test_tool", "key", serde_json::json!(1))
            .await
            .unwrap();
        memory
            .set("test_tool", "key", serde_json::json!(2))
            .await
            .unwrap();

        let value = memory.get("test_tool", "key").await.unwrap();
        assert_eq!(value, Some(serde_json::json!(2)));
    }

    #[tokio::test]
    async fn test_list_keys() {
        let (memory, _dir) = create_test_memory().await;

        memory
            .set("test_tool", "alpha", serde_json::json!(1))
            .await
            .unwrap();
        memory
            .set("test_tool", "beta", serde_json::json!(2))
            .await
            .unwrap();
        memory
            .set("test_tool", "gamma", serde_json::json!(3))
            .await
            .unwrap();

        let keys = memory.list_keys("test_tool").await.unwrap();
        assert_eq!(keys, vec!["alpha", "beta", "gamma"]);
    }

    #[tokio::test]
    async fn test_delete() {
        let (memory, _dir) = create_test_memory().await;

        memory
            .set("test_tool", "key", serde_json::json!(1))
            .await
            .unwrap();
        let deleted = memory.delete("test_tool", "key").await.unwrap();
        assert!(deleted);

        let value = memory.get("test_tool", "key").await.unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_clear() {
        let (memory, _dir) = create_test_memory().await;

        memory
            .set("test_tool", "key1", serde_json::json!(1))
            .await
            .unwrap();
        memory
            .set("test_tool", "key2", serde_json::json!(2))
            .await
            .unwrap();
        memory
            .set("other_tool", "key1", serde_json::json!(3))
            .await
            .unwrap();

        let cleared = memory.clear("test_tool").await.unwrap();
        assert_eq!(cleared, 2);

        let keys = memory.list_keys("test_tool").await.unwrap();
        assert!(keys.is_empty());

        // Other tool's data should be intact
        let value = memory.get("other_tool", "key1").await.unwrap();
        assert_eq!(value, Some(serde_json::json!(3)));
    }

    #[tokio::test]
    async fn test_complex_values() {
        let (memory, _dir) = create_test_memory().await;

        let complex = serde_json::json!({
            "name": "Test",
            "items": [1, 2, 3],
            "nested": {
                "deep": true
            }
        });

        memory
            .set("test_tool", "complex", complex.clone())
            .await
            .unwrap();
        let value = memory.get("test_tool", "complex").await.unwrap();

        assert_eq!(value, Some(complex));
    }

    #[tokio::test]
    async fn test_stats() {
        let (memory, _dir) = create_test_memory().await;

        memory
            .set("tool1", "key1", serde_json::json!(1))
            .await
            .unwrap();
        memory
            .set("tool1", "key2", serde_json::json!(2))
            .await
            .unwrap();
        memory
            .set("tool2", "key1", serde_json::json!(3))
            .await
            .unwrap();

        let stats = memory.stats().await.unwrap();
        assert_eq!(stats.total_entries, 3);
        assert_eq!(stats.total_tools, 2);
        assert_eq!(stats.schema_version, SCHEMA_VERSION);
    }

    #[tokio::test]
    async fn test_isolation_between_tools() {
        let (memory, _dir) = create_test_memory().await;

        memory
            .set("tool_a", "shared_key", serde_json::json!("A"))
            .await
            .unwrap();
        memory
            .set("tool_b", "shared_key", serde_json::json!("B"))
            .await
            .unwrap();

        let value_a = memory.get("tool_a", "shared_key").await.unwrap();
        let value_b = memory.get("tool_b", "shared_key").await.unwrap();

        assert_eq!(value_a, Some(serde_json::json!("A")));
        assert_eq!(value_b, Some(serde_json::json!("B")));
    }

    #[tokio::test]
    async fn test_get_all() {
        let (memory, _dir) = create_test_memory().await;

        memory
            .set("test_tool", "a", serde_json::json!(1))
            .await
            .unwrap();
        memory
            .set("test_tool", "b", serde_json::json!(2))
            .await
            .unwrap();

        let all = memory.get_all("test_tool").await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0], ("a".to_string(), serde_json::json!(1)));
        assert_eq!(all[1], ("b".to_string(), serde_json::json!(2)));
    }

    #[tokio::test]
    async fn test_ttl_expiration() {
        let (memory, _dir) = create_test_memory().await;

        // Set with 1 second TTL
        memory
            .set_with_ttl("test_tool", "temp", serde_json::json!("expires"), Some(1))
            .await
            .unwrap();

        // Should exist immediately
        let value = memory.get("test_tool", "temp").await.unwrap();
        assert!(value.is_some());

        // Wait for expiration
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Should be expired now
        let value = memory.get("test_tool", "temp").await.unwrap();
        assert!(value.is_none());
    }

    #[tokio::test]
    async fn test_no_ttl_never_expires() {
        let (memory, _dir) = create_test_memory().await;

        // Set without TTL
        memory
            .set_with_ttl("test_tool", "permanent", serde_json::json!("forever"), None)
            .await
            .unwrap();

        // Set with TTL=0 (should also never expire)
        memory
            .set_with_ttl(
                "test_tool",
                "also_permanent",
                serde_json::json!("forever"),
                Some(0),
            )
            .await
            .unwrap();

        let val1 = memory.get("test_tool", "permanent").await.unwrap();
        let val2 = memory.get("test_tool", "also_permanent").await.unwrap();

        assert!(val1.is_some());
        assert!(val2.is_some());
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let (memory, _dir) = create_test_memory().await;

        // Set one permanent, one expiring
        memory
            .set("test_tool", "permanent", serde_json::json!(1))
            .await
            .unwrap();
        memory
            .set_with_ttl("test_tool", "temp", serde_json::json!(2), Some(1))
            .await
            .unwrap();

        // Wait for expiration
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Cleanup
        let cleaned = memory.cleanup_expired().await.unwrap();
        assert_eq!(cleaned, 1);

        // Permanent should still exist
        let value = memory.get("test_tool", "permanent").await.unwrap();
        assert!(value.is_some());
    }
}
