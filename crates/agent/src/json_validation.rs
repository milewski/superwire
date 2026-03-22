use jsonschema::error::ValidationErrorKind;
use schemars::Schema;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct JsonValidationIssue {
    pub instance_path: String,
    pub message: String,
    is_fully_qualified: bool,
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct JsonValidationError {
    message: String,
    issues: Vec<JsonValidationIssue>,
}

pub fn validate_json_against_schema_with_context(
    instance: &Value,
    schema: &Schema,
    validation_context: &str,
) -> Result<(), JsonValidationError> {
    let serialized_schema = serde_json::to_value(schema).map_err(|error| JsonValidationError {
        message: format!("Failed to serialize schema for validation: {error}"),
        issues: Vec::new(),
    })?;

    let validator = jsonschema::validator_for(&serialized_schema).map_err(|error| JsonValidationError {
        message: format!("Failed to compile schema for validation: {error}"),
        issues: Vec::new(),
    })?;

    let mut validation_issues = Vec::new();

    for validation_error in validator.iter_errors(instance) {
        collect_validation_issues(&validation_error, &mut validation_issues);
    }

    if validation_issues.is_empty() {
        return Ok(());
    }

    validation_issues.sort_by(|left_issue, right_issue| {
        left_issue
            .instance_path
            .cmp(&right_issue.instance_path)
            .then_with(|| left_issue.message.cmp(&right_issue.message))
    });

    validation_issues.dedup_by(|left_issue, right_issue| {
        left_issue.instance_path == right_issue.instance_path && left_issue.message == right_issue.message
    });

    let formatted_issues = validation_issues
        .iter()
        .map(format_issue_for_display)
        .collect::<Vec<_>>()
        .join("\n");

    Err(JsonValidationError {
        message: format!("{validation_context}:\n{formatted_issues}"),
        issues: validation_issues,
    })
}

fn collect_validation_issues(validation_error: &jsonschema::ValidationError<'_>, validation_issues: &mut Vec<JsonValidationIssue>) {
    match validation_error.kind() {
        ValidationErrorKind::OneOfNotValid { context } => {
            collect_best_branch_context_errors(validation_error, validation_issues, context);
        }
        ValidationErrorKind::AnyOf { context } => {
            collect_best_branch_context_errors(validation_error, validation_issues, context);
        }
        _ => {
            add_validation_issue(validation_error, validation_issues);
        }
    }
}

fn collect_best_branch_context_errors(
    validation_error: &jsonschema::ValidationError<'_>,
    validation_issues: &mut Vec<JsonValidationIssue>,
    context: &[Vec<jsonschema::ValidationError<'static>>],
) {
    if context.is_empty() {
        add_validation_issue(validation_error, validation_issues);

        return;
    }

    let mut best_branch_issues: Option<Vec<JsonValidationIssue>> = None;
    let mut best_branch_score = usize::MAX;
    let mut best_branch_issue_count = usize::MAX;

    for branch_errors in context {
        if branch_errors.is_empty() {
            continue;
        }

        let mut branch_issues = Vec::new();

        for branch_error in branch_errors {
            collect_validation_issues(branch_error, &mut branch_issues);
        }

        let branch_score = score_issues(&branch_issues);
        let branch_issue_count = branch_issues.len();

        let is_better_branch =
            branch_score < best_branch_score || (branch_score == best_branch_score && branch_issue_count < best_branch_issue_count);

        if is_better_branch {
            best_branch_score = branch_score;
            best_branch_issue_count = branch_issue_count;
            best_branch_issues = Some(branch_issues);
        }
    }

    if let Some(best_issues) = best_branch_issues {
        validation_issues.extend(best_issues);

        return;
    }

    add_validation_issue(validation_error, validation_issues);
}

fn add_validation_issue(validation_error: &jsonschema::ValidationError<'_>, validation_issues: &mut Vec<JsonValidationIssue>) {
    let instance_path = format_instance_path(&validation_error.instance_path().to_string());
    let (issue_message, is_fully_qualified) = format_issue_message(&instance_path, &validation_error.to_string());

    validation_issues.push(JsonValidationIssue {
        instance_path,
        message: issue_message,
        is_fully_qualified,
    });
}

fn format_issue_for_display(validation_issue: &JsonValidationIssue) -> String {
    if validation_issue.is_fully_qualified || validation_issue.instance_path == "$" {
        return format!("- {}", validation_issue.message);
    }

    format!("- {}: {}", validation_issue.instance_path, validation_issue.message)
}

fn format_issue_message(instance_path: &str, validation_message: &str) -> (String, bool) {
    if let Some(required_property) = extract_required_property(validation_message) {
        let dotted_path = if instance_path == "$" {
            required_property.to_string()
        } else {
            format!("{instance_path}.{required_property}")
        };

        return (format!("{dotted_path} is required"), true);
    }

    (validation_message.to_string(), false)
}

fn extract_required_property(validation_message: &str) -> Option<&str> {
    let required_suffix = " is a required property";

    if !validation_message.ends_with(required_suffix) {
        return None;
    }

    let first_quote_index = validation_message.find('"')?;
    let remaining = &validation_message[first_quote_index + 1..];
    let second_quote_index = remaining.find('"')?;

    Some(&remaining[..second_quote_index])
}

fn score_issues(validation_issues: &[JsonValidationIssue]) -> usize {
    validation_issues
        .iter()
        .map(|validation_issue| score_message(&validation_issue.message))
        .sum()
}

fn score_message(message: &str) -> usize {
    if message.contains("is a required property") {
        return 1;
    }

    if message.contains("was expected") {
        return 2;
    }

    if message.contains("Additional properties are not allowed") {
        return 3;
    }

    4
}

fn format_instance_path(instance_path: &str) -> String {
    if instance_path.is_empty() {
        return "$".to_string();
    }

    instance_path
        .trim_start_matches('/')
        .split('/')
        .filter(|path_segment| !path_segment.is_empty())
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::validate_json_against_schema_with_context;
    use schemars::schema_for;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
    #[serde(tag = "type", rename_all = "snake_case")]
    enum FinalizeState {
        Success { answer: Person },
        Failure { reason: String },
    }

    #[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
    struct FinalizeArguments {
        output: FinalizeState,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct Person {
        age: i32,
        quote: String,
    }

    #[test]
    fn reports_property_level_schema_issues() {
        let schema = schema_for!(FinalizeArguments);
        let invalid_payload = serde_json::json!({
            "output": {
                "output": {
                    "output": {
                        "age": 35,
                        "quote": "The winter is coming."
                    },
                    "type": "success"
                }
            }
        });

        let validation_error = validate_json_against_schema_with_context(&invalid_payload, &schema, "JSON does not match schema")
            .expect_err("payload should fail schema validation");

        assert!(validation_error.to_string().contains("JSON does not match schema"));
        assert!(!validation_error.issues.is_empty());

        let has_output_path_issue = validation_error.issues.iter().any(|issue| issue.instance_path.contains("output"));

        assert!(has_output_path_issue);

        let has_type_or_output_issue = validation_error
            .issues
            .iter()
            .any(|issue| issue.message.contains("type") || issue.message.contains("output") || issue.message.contains("answer"));

        assert!(has_type_or_output_issue);
    }

    #[test]
    fn expands_one_of_with_missing_field_details() {
        let schema = schema_for!(FinalizeArguments);
        let invalid_payload = serde_json::json!({
            "output": {
                "type": "success",
                "answer": {
                    "quote": "The winter is coming."
                }
            }
        });

        let validation_error = validate_json_against_schema_with_context(&invalid_payload, &schema, "JSON does not match schema")
            .expect_err("payload should fail schema validation");

        assert!(validation_error.to_string().contains("age"));
        assert!(!validation_error
            .to_string()
            .contains("is not valid under any of the schemas listed in the 'oneOf' keyword"));

        assert!(!validation_error.to_string().contains("oneOf option"));
        assert!(!validation_error.to_string().contains("\"failure\" was expected"));
    }

    #[test]
    fn formats_required_keys_using_dotted_paths() {
        let schema = schema_for!(FinalizeArguments);
        let invalid_payload = serde_json::json!({
            "output": {
                "answer": {
                    "quote": "The winter is coming."
                }
            }
        });

        let validation_error = validate_json_against_schema_with_context(&invalid_payload, &schema, "JSON does not match schema")
            .expect_err("payload should fail schema validation");

        assert!(validation_error.to_string().contains("output.type is required"));
        assert!(validation_error.to_string().contains("output.answer.age is required"));
    }

    #[test]
    fn reports_missing_wrapper_keys_with_dotted_paths() {
        let schema = schema_for!(FinalizeArguments);
        let invalid_payload = serde_json::json!({
            "output": {}
        });

        let validation_error = validate_json_against_schema_with_context(&invalid_payload, &schema, "JSON does not match schema")
            .expect_err("payload should fail schema validation");

        assert!(validation_error.to_string().contains("output.answer is required"));
        assert!(validation_error.to_string().contains("output.type is required"));
    }
}
