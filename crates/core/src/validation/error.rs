use thiserror::Error;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("{}", format_duplicate_name(.file_path, *line, *column, .name, .first_defined_at, .suggestion))]
    DuplicateName {
        file_path: String,
        line: usize,
        column: usize,
        name: String,
        first_defined_at: String,
        suggestion: Option<String>,
    },

    #[error("{}", format_undefined_reference(.file_path, *line, *column, .reference, .suggestion))]
    UndefinedReference {
        file_path: String,
        line: usize,
        column: usize,
        reference: String,
        suggestion: Option<String>,
    },

    #[error("{}", format_provider_model_mismatch(.file_path, *line, *column, .message, .suggestion))]
    ProviderModelMismatch {
        file_path: String,
        line: usize,
        column: usize,
        message: String,
        suggestion: Option<String>,
    },

    #[error("{}", format_missing_template_variable(.file_path, *line, *column, .variable, .suggestion))]
    MissingTemplateVariable {
        file_path: String,
        line: usize,
        column: usize,
        variable: String,
        suggestion: Option<String>,
    },

    #[error("{}", format_unused_template_binding(.file_path, *line, *column, .binding, .suggestion))]
    UnusedTemplateBinding {
        file_path: String,
        line: usize,
        column: usize,
        binding: String,
        suggestion: Option<String>,
    },

    #[error("{}", format_invalid_property(.file_path, *line, *column, .property, .suggestion))]
    InvalidProperty {
        file_path: String,
        line: usize,
        column: usize,
        property: String,
        suggestion: Option<String>,
    },

    #[error("{}", format_missing_required_property(.file_path, *line, *column, .agent_name, .property_name, .suggestion))]
    MissingRequiredProperty {
        file_path: String,
        line: usize,
        column: usize,
        agent_name: String,
        property_name: String,
        suggestion: Option<String>,
    },

    #[error("{}", format_cyclic_dependency(.file_path, .cycle, .suggestion))]
    CyclicDependency {
        file_path: String,
        cycle: String,
        suggestion: Option<String>,
    },

    #[error("{}", format_invalid_input_output(.file_path, *line, *column, .message, .suggestion))]
    InvalidInputOutput {
        file_path: String,
        line: usize,
        column: usize,
        message: String,
        suggestion: Option<String>,
    },
}

fn format_duplicate_name(
    file_path: &str,
    line: usize,
    column: usize,
    name: &str,
    first_defined_at: &str,
    suggestion: &Option<String>,
) -> String {
    let mut result = format!(
        "Error: duplicate name '{}' (first defined at {})\n  --> {}:{}:{}\n   |",
        name, first_defined_at, file_path, line, column
    );

    if let Some(suggestion_text) = suggestion {
        result.push_str(&format!("\n   = help: {}", suggestion_text));
    }

    result
}

fn format_undefined_reference(
    file_path: &str,
    line: usize,
    column: usize,
    reference: &str,
    suggestion: &Option<String>,
) -> String {
    let mut result = format!(
        "Error: undefined reference '{}'\n  --> {}:{}:{}\n   |",
        reference, file_path, line, column
    );

    if let Some(suggestion_text) = suggestion {
        result.push_str(&format!("\n   = help: {}", suggestion_text));
    }

    result
}

fn format_provider_model_mismatch(
    file_path: &str,
    line: usize,
    column: usize,
    message: &str,
    suggestion: &Option<String>,
) -> String {
    let mut result = format!(
        "Error: provider/model mismatch: {}\n  --> {}:{}:{}\n   |",
        message, file_path, line, column
    );

    if let Some(suggestion_text) = suggestion {
        result.push_str(&format!("\n   = help: {}", suggestion_text));
    }

    result
}

fn format_missing_template_variable(
    file_path: &str,
    line: usize,
    column: usize,
    variable: &str,
    suggestion: &Option<String>,
) -> String {
    let mut result = format!(
        "Error: missing template variable '{}'\n  --> {}:{}:{}\n   |",
        variable, file_path, line, column
    );

    if let Some(suggestion_text) = suggestion {
        result.push_str(&format!("\n   = help: {}", suggestion_text));
    }

    result
}

fn format_unused_template_binding(
    file_path: &str,
    line: usize,
    column: usize,
    binding: &str,
    suggestion: &Option<String>,
) -> String {
    let mut result = format!(
        "Error: unused template binding '{}'\n  --> {}:{}:{}\n   |",
        binding, file_path, line, column
    );

    if let Some(suggestion_text) = suggestion {
        result.push_str(&format!("\n   = help: {}", suggestion_text));
    }

    result
}

fn format_invalid_property(
    file_path: &str,
    line: usize,
    column: usize,
    property: &str,
    suggestion: &Option<String>,
) -> String {
    let mut result = format!(
        "Error: invalid property '{}'\n  --> {}:{}:{}\n   |",
        property, file_path, line, column
    );

    if let Some(suggestion_text) = suggestion {
        result.push_str(&format!("\n   = help: {}", suggestion_text));
    }

    result
}

fn format_missing_required_property(
    file_path: &str,
    line: usize,
    column: usize,
    agent_name: &str,
    property_name: &str,
    suggestion: &Option<String>,
) -> String {
    let mut result = format!(
        "Error: missing required property '{}'\n  --> {}:{}:{}\n   |",
        property_name, file_path, line, column
    );

    result.push_str(&format!("\n   = note: agent '{}' requires this property", agent_name));

    if let Some(suggestion_text) = suggestion {
        result.push_str(&format!("\n   = help: {}", suggestion_text));
    }

    result
}

fn format_cyclic_dependency(file_path: &str, cycle: &str, suggestion: &Option<String>) -> String {
    let mut result = format!(
        "Error: cyclic dependency detected\n  --> {}\n   |\n   = note: {}",
        file_path, cycle
    );

    if let Some(suggestion_text) = suggestion {
        result.push_str(&format!("\n   = help: {}", suggestion_text));
    }

    result
}

fn format_invalid_input_output(
    file_path: &str,
    line: usize,
    column: usize,
    message: &str,
    suggestion: &Option<String>,
) -> String {
    let mut result = format!(
        "Error: invalid input/output: {}\n  --> {}:{}:{}\n   |",
        message, file_path, line, column
    );

    if let Some(suggestion_text) = suggestion {
        result.push_str(&format!("\n   = help: {}", suggestion_text));
    }

    result
}
