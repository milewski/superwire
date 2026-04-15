//! Execution policy for enforcing tool capabilities and sandboxing

use crate::{ToolCapabilities, ToolError, ToolResult};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Execution policy that enforces capability restrictions
pub struct ExecutionPolicy {
    capabilities: ToolCapabilities,
    start_time: Instant,
    allowlist_dirty: bool,
}

impl ExecutionPolicy {
    #[must_use]
    pub fn new(capabilities: ToolCapabilities) -> Self {
        Self {
            capabilities,
            start_time: Instant::now(),
            allowlist_dirty: false,
        }
    }

    #[must_use]
    pub fn with_strict_defaults() -> Self {
        Self::new(ToolCapabilities::no_access())
    }

    pub fn check_network_access(&self, url: &str) -> ToolResult<()> {
        if !self.capabilities.network_access {
            return Err(ToolError::BackendError(format!(
                "Network access denied: {url} not allowed by policy"
            )));
        }
        Ok(())
    }

    pub fn check_filesystem_access(&self, path: &PathBuf) -> ToolResult<()> {
        if !self.capabilities.filesystem_access {
            return Err(ToolError::BackendError(format!(
                "Filesystem access denied: {} not allowed by policy",
                path.display()
            )));
        }
        Ok(())
    }

    pub fn check_timeout(&self) -> ToolResult<()> {
        if let Some(timeout) = self.capabilities.timeout_seconds {
            let elapsed = self.start_time.elapsed();
            if elapsed > Duration::from_secs(timeout) {
                return Err(ToolError::BackendError(format!("Execution timeout: exceeded {timeout} seconds")));
            }
        }
        Ok(())
    }

    pub fn check_environment_variable(&self, name: &str) -> ToolResult<()> {
        if self.capabilities.allow_environment_variables.is_empty() {
            return Err(ToolError::BackendError(format!(
                "Environment variable access denied: {name} not in allowlist"
            )));
        }

        if !self.capabilities.allow_environment_variables.contains(&name.to_string()) {
            return Err(ToolError::BackendError(format!("Environment variable {name} not in allowlist")));
        }

        Ok(())
    }

    #[must_use]
    pub fn capabilities(&self) -> &ToolCapabilities {
        &self.capabilities
    }

    #[must_use]
    pub fn is_network_allowed(&self) -> bool {
        self.capabilities.network_access
    }

    #[must_use]
    pub fn is_filesystem_allowed(&self) -> bool {
        self.capabilities.filesystem_access
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_deny() {
        let policy = ExecutionPolicy::with_strict_defaults();
        let result = policy.check_network_access("https://api.example.com");
        assert!(result.is_err());
    }

    #[test]
    fn test_network_allow() {
        let mut caps = ToolCapabilities::no_access();
        caps.network_access = true;
        let policy = ExecutionPolicy::new(caps);
        let result = policy.check_network_access("https://api.example.com");
        assert!(result.is_ok());
    }

    #[test]
    fn test_env_deny() {
        let policy = ExecutionPolicy::with_strict_defaults();
        let result = policy.check_environment_variable("PATH");
        assert!(result.is_err());
    }

    #[test]
    fn test_env_allow() {
        let mut caps = ToolCapabilities::no_access();
        caps.allow_environment_variables = vec!["PATH".to_string()];
        let policy = ExecutionPolicy::new(caps);
        let result = policy.check_environment_variable("PATH");
        assert!(result.is_ok());
    }
}
