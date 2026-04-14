use std::path::{Path, PathBuf};

use crate::diagnostics::CommandError;

mod languages;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ToolProjectLanguage {
    Rust,
}

impl ToolProjectLanguage {
    fn from_identifier(identifier: &str) -> Option<Self> {
        match identifier {
            "rust" => Some(Self::Rust),
            _ => None,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
        }
    }

    fn supported_languages() -> String {
        [Self::Rust].into_iter().map(Self::as_str).collect::<Vec<_>>().join(", ")
    }
}

pub trait ToolLanguageScaffolder {
    fn scaffold(&self, project_directory: &Path) -> Result<Vec<PathBuf>, CommandError>;
}

pub fn scaffold_tool_project(language: &str, project_directory: &Path) -> Result<Vec<PathBuf>, CommandError> {
    let target_language = ToolProjectLanguage::from_identifier(language).ok_or_else(|| {
        CommandError::invalid_input(format!(
            "unsupported language `{language}`. supported languages: {}",
            ToolProjectLanguage::supported_languages()
        ))
    })?;

    let language_scaffolder = languages::scaffolder_for(target_language);

    language_scaffolder.scaffold(project_directory)
}
