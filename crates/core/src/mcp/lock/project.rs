use super::{McpLock, ProjectMcpLock, ProjectWorkflowMcpLockEntry, PROJECT_MCP_LOCK_FILE_NAME};
use crate::mcp::McpError;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

impl ProjectMcpLock {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            version: 1,
            workflows: BTreeMap::new(),
        }
    }

    pub fn read_from_path(lock_path: &Path) -> Result<Self, McpError> {
        let lock_text = std::fs::read_to_string(lock_path).map_err(|source| McpError::ReadLock {
            path: lock_path.display().to_string(),
            source,
        })?;

        serde_json::from_str(&lock_text).map_err(|source| McpError::ParseLock {
            path: lock_path.display().to_string(),
            source,
        })
    }

    pub fn write_to_path(&self, lock_path: &Path) -> Result<(), McpError> {
        let lock_text = serde_json::to_string_pretty(self).map_err(|source| McpError::SerializeLock {
            path: lock_path.display().to_string(),
            source,
        })?;

        std::fs::write(lock_path, format!("{lock_text}\n")).map_err(|source| McpError::WriteLock {
            path: lock_path.display().to_string(),
            source,
        })
    }

    pub fn insert_workflow_lock(&mut self, lock_root: &Path, workflow_path: &Path, workflow_lock: McpLock) {
        self.insert_workflow_lock_with_source(lock_root, workflow_path, workflow_lock, "");
    }

    pub fn insert_workflow_lock_with_source(
        &mut self,
        lock_root: &Path,
        workflow_path: &Path,
        workflow_lock: McpLock,
        workflow_source: &str,
    ) {
        let workflow_key = Self::workflow_key(lock_root, workflow_path);
        let workflow_hash = Self::workflow_hash(workflow_source);

        self.workflows.insert(
            workflow_key,
            ProjectWorkflowMcpLockEntry {
                hash: workflow_hash,
                lock: workflow_lock,
            },
        );
    }

    #[must_use]
    pub fn workflow_lock(&self, lock_root: &Path, workflow_path: &Path) -> Option<&McpLock> {
        let workflow_key = Self::workflow_key(lock_root, workflow_path);

        self.workflows.get(&workflow_key).map(ProjectWorkflowMcpLockEntry::lock)
    }

    #[must_use]
    pub fn discover_lock_path_for_workflow(workflow_path: &Path) -> Option<PathBuf> {
        let mut current_directory = if workflow_path.is_dir() {
            workflow_path.to_path_buf()
        } else {
            workflow_path.parent()?.to_path_buf()
        };

        loop {
            let candidate_path = current_directory.join(PROJECT_MCP_LOCK_FILE_NAME);

            if candidate_path.exists() {
                return Some(candidate_path);
            }

            if !current_directory.pop() {
                return None;
            }
        }
    }

    fn workflow_key(lock_root: &Path, workflow_path: &Path) -> String {
        let normalized_workflow_path = workflow_path.canonicalize().unwrap_or_else(|_error| workflow_path.to_path_buf());
        let lock_root_path = if lock_root.as_os_str().is_empty() {
            Path::new(".")
        } else {
            lock_root
        };
        let normalized_lock_root = lock_root_path.canonicalize().unwrap_or_else(|_error| lock_root_path.to_path_buf());
        let relative_workflow_path = normalized_workflow_path
            .strip_prefix(&normalized_lock_root)
            .unwrap_or(normalized_workflow_path.as_path());

        relative_workflow_path.to_string_lossy().replace('\\', "/")
    }

    fn workflow_hash(workflow_source: &str) -> String {
        format!("{:x}", Sha256::digest(workflow_source.as_bytes()))
    }
}

impl ProjectWorkflowMcpLockEntry {
    #[must_use]
    pub fn lock(&self) -> &McpLock {
        &self.lock
    }
}
