use crate::error::WorkflowError;
use regex::Regex;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateDocument {
    pub path: PathBuf,
    pub placeholders: BTreeSet<String>,
    pub source: String,
}

impl TemplateDocument {
    pub fn load(base_path: &Path, relative_path: &str) -> Result<Self, WorkflowError> {
        let template_path = base_path.join(relative_path);
        let source = std::fs::read_to_string(&template_path)?;
        let placeholder_regex = Regex::new(r"\{\{\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*\}\}")
            .map_err(|error| WorkflowError::parse(format!("failed to compile template regex: {error}")))?;
        let placeholders = placeholder_regex
            .captures_iter(&source)
            .map(|captures| captures[1].to_string())
            .collect::<BTreeSet<_>>();

        Ok(Self {
            path: template_path,
            placeholders,
            source,
        })
    }
}
