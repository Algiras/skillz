//! Hot Reload - File system watcher for automatic tool reloading
//! 
//! Watches the tools directory for changes and automatically reloads
//! modified tools without server restart.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebouncedEvent};

/// Events emitted by the file watcher
#[derive(Debug, Clone)]
pub enum WatchEvent {
    /// A tool was modified (manifest.json or script changed)
    ToolModified(String),
    /// A new tool was added
    ToolAdded(String),
    /// A tool was removed
    ToolRemoved(String),
    /// Watcher error
    Error(String),
}

/// File watcher for hot reload functionality
pub struct ToolWatcher {
    tools_dir: PathBuf,
    event_tx: mpsc::Sender<WatchEvent>,
}

impl ToolWatcher {
    /// Create a new tool watcher
    pub fn new(tools_dir: PathBuf, event_tx: mpsc::Sender<WatchEvent>) -> Self {
        Self { tools_dir, event_tx }
    }

    /// Start watching the tools directory
    /// Returns a handle that can be used to stop watching
    pub async fn start(self) -> anyhow::Result<WatchHandle> {
        let tools_dir = self.tools_dir.clone();
        let event_tx = self.event_tx.clone();
        
        // Create a channel for the notify events
        let (tx, rx) = std::sync::mpsc::channel();
        
        // Create debounced watcher (debounces rapid file changes)
        let mut debouncer = new_debouncer(Duration::from_millis(500), tx)?;
        
        // Watch the tools directory recursively
        debouncer.watcher().watch(&tools_dir, RecursiveMode::Recursive)?;
        
        eprintln!("🔥 Hot reload enabled - watching {}", tools_dir.display());
        
        // Spawn a task to handle file events
        let handle = tokio::task::spawn_blocking(move || {
            Self::event_loop(rx, tools_dir, event_tx)
        });
        
        Ok(WatchHandle {
            _debouncer: debouncer,
            _handle: handle,
        })
    }
    
    /// Event loop that processes file system events
    fn event_loop(
        rx: std::sync::mpsc::Receiver<Result<Vec<DebouncedEvent>, notify_debouncer_mini::notify::Error>>,
        tools_dir: PathBuf,
        event_tx: mpsc::Sender<WatchEvent>,
    ) {
        loop {
            match rx.recv() {
                Ok(Ok(events)) => {
                    for event in events {
                        if let Some(watch_event) = Self::process_event(&event, &tools_dir) {
                            // Use blocking send since we're in a sync context
                            let _ = event_tx.blocking_send(watch_event);
                        }
                    }
                }
                Ok(Err(e)) => {
                    let _ = event_tx.blocking_send(WatchEvent::Error(e.to_string()));
                }
                Err(_) => {
                    // Channel closed, watcher stopped
                    break;
                }
            }
        }
    }
    
    /// Process a single file system event and convert to WatchEvent
    fn process_event(event: &DebouncedEvent, tools_dir: &PathBuf) -> Option<WatchEvent> {
        let path = &event.path;
        
        // Skip if not in tools directory
        if !path.starts_with(tools_dir) {
            return None;
        }
        
        // Get relative path from tools dir
        let rel_path = path.strip_prefix(tools_dir).ok()?;
        
        // Extract tool name (first component of path)
        let tool_name = rel_path.components().next()?.as_os_str().to_str()?.to_string();
        
        // Skip hidden files and temp files
        if tool_name.starts_with('.') || tool_name.starts_with('_') {
            return None;
        }
        
        // Check what file changed
        let file_name = path.file_name()?.to_str()?;
        
        // Only care about manifest.json, scripts, and wasm files
        let is_relevant = file_name == "manifest.json"
            || file_name.ends_with(".py")
            || file_name.ends_with(".js")
            || file_name.ends_with(".rb")
            || file_name.ends_with(".sh")
            || file_name.ends_with(".wasm");
        
        if !is_relevant {
            return None;
        }
        
        // Determine event type based on file existence
        let tool_dir = tools_dir.join(&tool_name);
        let manifest_path = tool_dir.join("manifest.json");
        
        if !tool_dir.exists() {
            Some(WatchEvent::ToolRemoved(tool_name))
        } else if manifest_path.exists() {
            // Check if this is a new tool or modification
            // For simplicity, we treat all as modifications since the registry
            // will handle both cases
            Some(WatchEvent::ToolModified(tool_name))
        } else {
            None
        }
    }
}

/// Handle to the running watcher
/// Dropping this handle will stop the watcher
pub struct WatchHandle {
    _debouncer: notify_debouncer_mini::Debouncer<notify_debouncer_mini::notify::RecommendedWatcher>,
    _handle: tokio::task::JoinHandle<()>,
}

/// Hot reload manager that coordinates watching and reloading
pub struct HotReload {
    event_rx: mpsc::Receiver<WatchEvent>,
    _watch_handle: Option<WatchHandle>,
}

impl HotReload {
    /// Create and start hot reload for the given tools directory
    pub async fn start(tools_dir: PathBuf) -> anyhow::Result<Self> {
        let (event_tx, event_rx) = mpsc::channel(100);
        
        let watcher = ToolWatcher::new(tools_dir, event_tx);
        let watch_handle = watcher.start().await?;
        
        Ok(Self {
            event_rx,
            _watch_handle: Some(watch_handle),
        })
    }
    
    /// Get the next watch event (non-blocking)
    pub async fn next_event(&mut self) -> Option<WatchEvent> {
        self.event_rx.recv().await
    }
    
    /// Try to get the next watch event (non-blocking, returns immediately)
    pub fn try_next_event(&mut self) -> Option<WatchEvent> {
        self.event_rx.try_recv().ok()
    }
}

/// Callback type for tool reload notifications
pub type ReloadCallback = Arc<dyn Fn(&str) + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::*;
    use notify_debouncer_mini::DebouncedEventKind;
    
    #[test]
    fn test_watch_event_types() {
        // Test that WatchEvent enum variants work correctly
        let modified = WatchEvent::ToolModified("test_tool".to_string());
        let added = WatchEvent::ToolAdded("new_tool".to_string());
        let removed = WatchEvent::ToolRemoved("old_tool".to_string());
        let error = WatchEvent::Error("test error".to_string());
        
        match modified {
            WatchEvent::ToolModified(name) => assert_eq!(name, "test_tool"),
            _ => panic!("Expected ToolModified"),
        }
        
        match added {
            WatchEvent::ToolAdded(name) => assert_eq!(name, "new_tool"),
            _ => panic!("Expected ToolAdded"),
        }
        
        match removed {
            WatchEvent::ToolRemoved(name) => assert_eq!(name, "old_tool"),
            _ => panic!("Expected ToolRemoved"),
        }
        
        match error {
            WatchEvent::Error(msg) => assert_eq!(msg, "test error"),
            _ => panic!("Expected Error"),
        }
    }
    
    #[test]
    fn test_process_event_filters_hidden() {
        use std::path::PathBuf;
        
        let tools_dir = PathBuf::from("/tools");
        
        // Hidden files should be filtered
        let hidden_event = DebouncedEvent {
            path: PathBuf::from("/tools/.hidden_tool/manifest.json"),
            kind: DebouncedEventKind::Any,
        };
        assert!(ToolWatcher::process_event(&hidden_event, &tools_dir).is_none());
        
        // Temp files should be filtered
        let temp_event = DebouncedEvent {
            path: PathBuf::from("/tools/_temp/script.py"),
            kind: DebouncedEventKind::Any,
        };
        assert!(ToolWatcher::process_event(&temp_event, &tools_dir).is_none());
    }
    
    #[test]
    fn test_process_event_filters_irrelevant() {
        use std::path::PathBuf;
        
        let tools_dir = PathBuf::from("/tools");
        
        // Non-relevant files should be filtered
        let readme_event = DebouncedEvent {
            path: PathBuf::from("/tools/my_tool/README.md"),
            kind: DebouncedEventKind::Any,
        };
        assert!(ToolWatcher::process_event(&readme_event, &tools_dir).is_none());
        
        // .txt files should be filtered
        let txt_event = DebouncedEvent {
            path: PathBuf::from("/tools/my_tool/notes.txt"),
            kind: DebouncedEventKind::Any,
        };
        assert!(ToolWatcher::process_event(&txt_event, &tools_dir).is_none());
    }
}

