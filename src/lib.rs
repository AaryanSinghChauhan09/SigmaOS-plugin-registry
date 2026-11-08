// SPDX-License-Identifier: Apache-2.0
//! SigmaOS-plugin-registry — SigmaOS plugin-registry component inspired by Linux Mint / Omarchy / Arch Linux.
//!
//! Plugin registry: name-keyed plugins behind a trait object, fail-fast duplicate/missing.
//! Principles: single responsibility, fail-fast, defensive programming,
//! component isolation, stable API, deterministic tests.

#![forbid(unsafe_code)]

use std::fmt;
/// Errors for `PluginError` operations. Fail-fast: invalid input is rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginError {
    /// Plugin name already registered
    Duplicate(String),
    /// Plugin not registered
    NotFound(String),
    /// Plugin name must not be empty
    EmptyName,
    /// Plugin execution failed
    ExecFailed(String),
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate(x) => write!(f, "plugin name already registered: {x}"),
            Self::NotFound(x) => write!(f, "plugin not registered: {x}"),
            Self::EmptyName => write!(f, "plugin name must not be empty"),
            Self::ExecFailed(x) => write!(f, "plugin execution failed: {x}"),
        }
    }
}

impl std::error::Error for PluginError {}

/// A plugin contributes a named action to the host.
pub trait Plugin {
    /// Unique plugin name.
    fn name(&self) -> &str;
    /// Execute the plugin's primary action.
    fn execute(&self) -> Result<String, PluginError>;
}

/// Registry of named plugins.
#[derive(Default)]
pub struct PluginRegistry {
    plugins: Vec<Box<dyn Plugin>>,
}

impl PluginRegistry {
    /// Empty registry.
    pub fn new() -> Self { Self::default() }

    /// Register a plugin; fails on empty name or duplicate.
    pub fn register(&mut self, plugin: Box<dyn Plugin>) -> Result<(), PluginError> {
        let n = plugin.name();
        if n.trim().is_empty() { return Err(PluginError::EmptyName); }
        if self.plugins.iter().any(|p| p.name() == n) {
            return Err(PluginError::Duplicate(n.to_string()));
        }
        self.plugins.push(plugin);
        Ok(())
    }

    /// Run a plugin by name; fails if absent.
    pub fn run(&self, name: &str) -> Result<String, PluginError> {
        self.plugins.iter().find(|p| p.name() == name)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))?
            .execute()
    }

    /// Plugin names in registration order.
    pub fn names(&self) -> Vec<&str> {
        self.plugins.iter().map(|p| p.name()).collect()
    }

    /// Number of plugins.
    pub fn len(&self) -> usize { self.plugins.len() }

    /// Whether empty.
    pub fn is_empty(&self) -> bool { self.plugins.is_empty() }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Echo;
    impl Plugin for Echo {
        fn name(&self) -> &str { "echo" }
        fn execute(&self) -> Result<String, PluginError> { Ok("echo: ok".into()) }
    }

    #[test]
    fn register_and_run() {
        let mut r = PluginRegistry::new();
        r.register(Box::new(Echo)).unwrap();
        assert_eq!(r.run("echo").unwrap(), "echo: ok");
    }

    #[test]
    fn duplicate_fails() {
        let mut r = PluginRegistry::new();
        r.register(Box::new(Echo)).unwrap();
        assert!(matches!(r.register(Box::new(Echo)), Err(PluginError::Duplicate(_))));
    }

    #[test]
    fn run_missing_fails() {
        assert!(matches!(PluginRegistry::new().run("x"), Err(PluginError::NotFound(_))));
    }

}

/// Backwards-compatible component handle (kept for existing callers/tests).
#[derive(Debug, Default)]
pub struct Component {
    name: &'static str,
}

impl Component {
    /// Create the component handle.
    pub fn new() -> Self {
        Self { name: "SigmaOS Component" }
    }
}
