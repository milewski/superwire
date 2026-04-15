//! Tool registry for managing tool lifecycle, discovery, and execution

use crate::{ToolBackend, ToolDescriptor, ToolError, ToolResult};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Unique tool identifier that combines name and source
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToolId {
    pub name: String,
    pub source_path: PathBuf,
}

impl std::fmt::Display for ToolId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.name, self.source_path.display())
    }
}

/// Type of tool backend
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolBackendType {
    Wasm,
    Native,
    Cli,
}

/// Metadata about a registered tool (without the actual backend)
#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub id: ToolId,
    pub descriptor: ToolDescriptor,
    pub backend_type: ToolBackendType,
    pub capabilities: crate::ToolCapabilities,
}

/// Tool registry that manages tool loading and lifecycle
pub struct ToolRegistry {
    tools_dir: PathBuf,
    cache: HashMap<ToolId, Arc<dyn ToolBackend>>,
    info_cache: HashMap<ToolId, ToolInfo>,
}

impl ToolRegistry {
    pub fn new<P: AsRef<Path>>(tools_dir: P) -> Self {
        Self {
            tools_dir: tools_dir.as_ref().to_path_buf(),
            cache: HashMap::new(),
            info_cache: HashMap::new(),
        }
    }

    pub fn discover_tools(&mut self) -> ToolResult<Vec<ToolInfo>> {
        let mut discovered = Vec::new();

        if !self.tools_dir.exists() {
            return Ok(discovered);
        }

        for entry in std::fs::read_dir(&self.tools_dir).map_err(ToolError::IoError)? {
            let entry = entry.map_err(ToolError::IoError)?;
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            let tool_info = self.inspect_tool_at_path(&path)?;
            if let Some(info) = tool_info {
                let id = info.id.clone();
                self.info_cache.insert(id.clone(), info.clone());
                discovered.push(info);
            }
        }

        Ok(discovered)
    }

    pub fn get_tool(&mut self, name: &str, source_path: &Path) -> ToolResult<Arc<dyn ToolBackend>> {
        let id = ToolId {
            name: name.to_string(),
            source_path: source_path.to_path_buf(),
        };

        if let Some(backend) = self.cache.get(&id) {
            return Ok(backend.clone());
        }

        let backend = self.load_tool_backend(&id)?;
        self.cache.insert(id.clone(), backend.clone());

        Ok(backend)
    }

    pub fn get_tool_info(&self, name: &str, source_path: &Path) -> ToolResult<Option<ToolInfo>> {
        let id = ToolId {
            name: name.to_string(),
            source_path: source_path.to_path_buf(),
        };

        Ok(self.info_cache.get(&id).cloned())
    }

    #[must_use]
    pub fn list_tools(&self) -> Vec<ToolInfo> {
        self.info_cache.values().cloned().collect()
    }

    fn inspect_tool_at_path(&self, path: &Path) -> ToolResult<Option<ToolInfo>> {
        let extension = path.extension().and_then(|ext| ext.to_str());

        let backend_type = match extension {
            Some("wasm") => ToolBackendType::Wasm,
            _ => return Ok(None),
        };

        let backend = self.load_backend_from_path(path, backend_type)?;
        let descriptor = backend.describe()?;

        let id = ToolId {
            name: descriptor.name.clone(),
            source_path: path.to_path_buf(),
        };

        let info = ToolInfo {
            id,
            descriptor,
            backend_type,
            capabilities: crate::ToolCapabilities::no_access(),
        };

        Ok(Some(info))
    }

    fn load_backend_from_path(&self, path: &Path, backend_type: ToolBackendType) -> ToolResult<Arc<dyn ToolBackend>> {
        match backend_type {
            ToolBackendType::Wasm => {
                let wasm_backend = crate::backend::wasm::WasmBackend::new(path)?;
                Ok(Arc::new(wasm_backend))
            }
            _ => Err(ToolError::BackendError(format!(
                "Unsupported backend type for path: {}",
                path.display()
            ))),
        }
    }

    fn load_tool_backend(&self, id: &ToolId) -> ToolResult<Arc<dyn ToolBackend>> {
        let info = self
            .get_tool_info(&id.name, &id.source_path)?
            .ok_or_else(|| ToolError::ToolNotFound(id.to_string()))?;

        self.load_backend_from_path(&id.source_path, info.backend_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_id_display() {
        let id = ToolId {
            name: "test-tool".to_string(),
            source_path: PathBuf::from("/tmp/tool.wasm"),
        };

        assert_eq!(id.to_string(), "test-tool@/tmp/tool.wasm");
    }
}
