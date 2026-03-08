use regex::Regex;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use crate::utils::error::UtilsError;

pub fn interpolate_template(template: &str, variables: &HashMap<String, Value>) -> Result<String, UtilsError> {
    let re = Regex::new(r"\{\{\s*([a-zA-Z_][a-zA-Z0-9_.]*)\s*\}\}")
        .map_err(|e| UtilsError::TemplateParse { message: e.to_string() })?;

    let mut result = template.to_string();
    let mut missing_vars = Vec::new();

    for cap in re.captures_iter(template) {
        let full_match = &cap[0];
        let var_path = &cap[1];

        let value = resolve_variable_path(var_path, variables);

        match value {
            Some(val) => {
                let replacement = value_to_string(&val);
                result = result.replace(full_match, &replacement);
            }
            None => {
                missing_vars.push(var_path.to_string());
            }
        }
    }

    if !missing_vars.is_empty() {
        return Err(UtilsError::MissingTemplateVariables {
            variables: missing_vars,
        });
    }

    Ok(result)
}

pub fn read_and_interpolate_file(file_path: &str, variables: &HashMap<String, Value>) -> Result<String, UtilsError> {
    let path = Path::new(file_path);
    let content = fs::read_to_string(path).map_err(|e| UtilsError::FileRead {
        message: format!("failed to read file '{}': {}", file_path, e),
    })?;

    validate_template_bindings(&content, variables)?;

    interpolate_template(&content, variables)
}

fn validate_template_bindings(template: &str, variables: &HashMap<String, Value>) -> Result<(), UtilsError> {
    let re = Regex::new(r"\{\{\s*([a-zA-Z_][a-zA-Z0-9_.]*)\s*\}\}")
        .map_err(|e| UtilsError::TemplateParse { message: e.to_string() })?;

    let mut template_vars = HashSet::new();
    for cap in re.captures_iter(template) {
        template_vars.insert(cap[1].to_string());
    }

    let provided_vars: HashSet<String> = variables.keys().cloned().collect();

    let missing: Vec<String> = template_vars.difference(&provided_vars).cloned().collect();
    if !missing.is_empty() {
        return Err(UtilsError::MissingTemplateVariables { variables: missing });
    }

    let unused: Vec<String> = provided_vars.difference(&template_vars).cloned().collect();
    if !unused.is_empty() {
        return Err(UtilsError::UnusedTemplateBindings { bindings: unused });
    }

    Ok(())
}

fn resolve_variable_path(path: &str, variables: &HashMap<String, Value>) -> Option<Value> {
    let parts: Vec<&str> = path.split('.').collect();

    if parts.is_empty() {
        return None;
    }

    let mut current = variables.get(parts[0])?.clone();

    for part in &parts[1..] {
        current = match current {
            Value::Object(map) => map.get(*part)?.clone(),
            Value::Array(arr) => {
                let index: usize = part.parse().ok()?;
                arr.get(index)?.clone()
            }
            _ => return None,
        };
    }

    Some(current)
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_simple_interpolation() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), json!("Alice"));

        let result = interpolate_template("Hello {{ name }}!", &vars).unwrap();
        assert_eq!(result, "Hello Alice!");
    }

    #[test]
    fn test_nested_interpolation() {
        let mut vars = HashMap::new();
        vars.insert("user".to_string(), json!({"name": "Bob", "age": 30}));

        let result = interpolate_template("User: {{ user.name }}, Age: {{ user.age }}", &vars).unwrap();
        assert_eq!(result, "User: Bob, Age: 30");
    }

    #[test]
    fn test_missing_variable() {
        let vars = HashMap::new();
        let result = interpolate_template("Hello {{ name }}!", &vars);
        assert!(result.is_err());
    }

    #[test]
    fn test_unused_binding_detection() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), json!("Alice"));
        vars.insert("unused".to_string(), json!("value"));

        let result = validate_template_bindings("Hello {{ name }}!", &vars);
        assert!(result.is_err());
    }
}
