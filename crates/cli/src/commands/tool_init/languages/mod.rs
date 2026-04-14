use super::{ToolLanguageScaffolder, ToolProjectLanguage};

mod rust;

pub fn scaffolder_for(language: ToolProjectLanguage) -> Box<dyn ToolLanguageScaffolder> {
    match language {
        ToolProjectLanguage::Rust => Box::new(rust::RustToolLanguageScaffolder::new()),
    }
}
