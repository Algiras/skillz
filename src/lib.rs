//! Skillz - Build and execute custom MCP tools at runtime
//!
//! This crate provides a Model Context Protocol (MCP) server that allows
//! dynamic tool creation using WebAssembly and script-based tools.

pub mod builder;
pub mod client;
pub mod config;
pub mod importer;
pub mod memory;
pub mod pipeline;
pub mod prompts;
pub mod registry;
pub mod runtime;
pub mod services;
pub mod watcher;
