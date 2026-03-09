use std::fmt::Write;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("{}", format_duplicate_name(.file_path, *line, *column, .name, .first_defined_at, .suggestion.as_ref()))]
    DuplicateName {
        file_path: String,
        line: usize,
        column: usize,
        name: String,
        first_defined_at: String,
        suggestion: Option<String>,
    },

    #[error("{}", format_undefined_reference(.file_path, *line, *column, .reference, .suggestion.as_ref()))]
    UndefinedReference {
        file_path: String,
        line: usize,
        column: usize,
        reference: String,
        suggestion: Option<String>,
    },

    #[error("{}", format_provider_model_mismatch(.file_path, *line, *column, .message, .suggestion.as_ref()))]
    ProviderModelMismatch {
        file_path: String,
        line: usize,
        column: usize,
        message: String,
        suggestion: Option<String>,
    },

    #[error("{}", format_missing_template_variable(.file_path, *line, *column, .variable, .suggestion.as_ref()))]
    MissingTemplateVariable {
        file_path: String,
        line: usize,
        column: usize,
        variable: String,
        suggestion: Option<String>,
    },

    #[error("{}", format_unused_template_binding(.file_path, *line, *column, .binding, .suggestion.as_ref()))]
    UnusedTemplateBinding {
        file_path: String,
        line: usize,
        column: usize,
        binding: String,
        suggestion: Option<String>,
    },

    #[error("{}", format_invalid_property(.file_path, *line, *column, .property, .suggestion.as_ref()))]
    InvalidProperty {
        file_path: String,
        line: usize,
        column: usize,
        property: String,
        suggestion: Option<String>,
    },

    #[error("{}", format_missing_required_property(.file_path, *line, *column, .agent_name, .property_name, .suggestion.as_ref()))]
    MissingRequiredProperty {
        file_path: String,
        line: usize,
        column: usize,
        agent_name: String,
        property_name: String,
        suggestion: Option<String>,
    },

    #[error("{}", format_cyclic_dependency(.file_path, .cycle, .suggestion.as_ref()))]
    CyclicDependency {
        file_path: String,
        cycle: String,
        suggestion: Option<String>,
    },

    #[error("{}", format_invalid_input_output(.file_path, *line, *column, .message, .suggestion.as_ref()))]
    InvalidInputOutput {
        file_path: String,
        line: usize,
        column: usize,
        message: String,
        suggestion: Option<String>,
    },

    #[error("{}", format_missing_required_argument(.file_path, *line, *column, .function_name, .argument_name, .suggestion.as_ref()))]
    MissingRequiredArgument {
        file_path: String,
        line: usize,
        column: usize,
        function_name: String,
        argument_name: String,
        suggestion: Option<String>,
    },
}

fn format_duplicate_name(
    file_path: &str,
    line: usize,
    column: usize,
    name: &str,
    first_defined_at: &str,
    suggestion: Option<&String>,
) -> String {
    let mut result = format!(
        "Error: duplicate name '{name}' (first defined at {first_defined_at})\n  --> {file_path}:{line}:{column}\n   |"
    );

    if let Some(suggestion_text) = suggestion {
        write!(result, "\n   = help: {suggestion_text}").unwrap();
    }

    result
}

fn format_undefined_reference(
    file_path: &str,
    line: usize,
    column: usize,
    reference: &str,
    suggestion: Option<&String>,
) -> String {
    let mut result = format!("Error: undefined reference '{reference}'\n  --> {file_path}:{line}:{column}\n   |");

    if let Some(suggestion_text) = suggestion {
        write!(result, "\n   = help: {suggestion_text}").unwrap();
    }

    result
}

fn format_provider_model_mismatch(
    file_path: &str,
    line: usize,
    column: usize,
    message: &str,
    suggestion: Option<&String>,
) -> String {
    let mut result = format!("Error: provider/model mismatch: {message}\n  --> {file_path}:{line}:{column}\n   |");

    if let Some(suggestion_text) = suggestion {
        write!(result, "\n   = help: {suggestion_text}").unwrap();
    }

    result
}

fn format_missing_template_variable(
    file_path: &str,
    line: usize,
    column: usize,
    variable: &str,
    suggestion: Option<&String>,
) -> String {
    let mut result = format!("Error: missing template variable '{variable}'\n  --> {file_path}:{line}:{column}\n   |");

    if let Some(suggestion_text) = suggestion {
        write!(result, "\n   = help: {suggestion_text}").unwrap();
    }

    result
}

fn format_unused_template_binding(
    file_path: &str,
    line: usize,
    column: usize,
    binding: &str,
    suggestion: Option<&String>,
) -> String {
    let mut result = format!("Error: unused template binding '{binding}'\n  --> {file_path}:{line}:{column}\n   |");

    if let Some(suggestion_text) = suggestion {
        write!(result, "\n   = help: {suggestion_text}").unwrap();
    }

    result
}

fn format_invalid_property(
    file_path: &str,
    line: usize,
    column: usize,
    property: &str,
    suggestion: Option<&String>,
) -> String {
    let mut result = format!("Error: invalid property '{property}'\n  --> {file_path}:{line}:{column}\n   |");

    if let Some(suggestion_text) = suggestion {
        write!(result, "\n   = help: {suggestion_text}").unwrap();
    }

    result
}

fn format_missing_required_property(
    file_path: &str,
    line: usize,
    column: usize,
    agent_name: &str,
    property_name: &str,
    suggestion: Option<&String>,
) -> String {
    let mut result =
        format!("Error: missing required property '{property_name}'\n  --> {file_path}:{line}:{column}\n   |");

    write!(result, "\n   = note: agent '{agent_name}' requires this property").unwrap();

    if let Some(suggestion_text) = suggestion {
        write!(result, "\n   = help: {suggestion_text}").unwrap();
    }

    result
}

fn format_cyclic_dependency(file_path: &str, cycle: &str, suggestion: Option<&String>) -> String {
    let mut result = format!("Error: cyclic dependency detected\n  --> {file_path}\n   |\n   = note: {cycle}");

    if let Some(suggestion_text) = suggestion {
        write!(result, "\n   = help: {suggestion_text}").unwrap();
    }

    result
}

fn format_invalid_input_output(
    file_path: &str,
    line: usize,
    column: usize,
    message: &str,
    suggestion: Option<&String>,
) -> String {
    let mut result = format!("Error: invalid input/output: {message}\n  --> {file_path}:{line}:{column}\n   |");

    if let Some(suggestion_text) = suggestion {
        write!(result, "\n   = help: {suggestion_text}").unwrap();
    }

    result
}

fn format_missing_required_argument(
    file_path: &str,
    line: usize,
    column: usize,
    function_name: &str,
    argument_name: &str,
    suggestion: Option<&String>,
) -> String {
    let mut result = format!(
        "Error: missing required argument '{argument_name}' in function '{function_name}'\n  --> {file_path}:{line}:{column}\n   |"
    );

    if let Some(suggestion_text) = suggestion {
        write!(result, "\n   = help: {suggestion_text}").unwrap();
    }

    result
}
