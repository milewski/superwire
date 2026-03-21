use crate::error::CliError;
use serde_json::Value;
use std::collections::HashMap;

pub fn parse_inputs(raw_inputs: Vec<String>) -> Result<HashMap<String, Value>, CliError> {
    let mut inputs = HashMap::new();
    let mut iterator = raw_inputs.iter();

    while let Some(argument) = iterator.next() {
        if !argument.starts_with("--") {
            return Err(CliError::InvalidArguments(format!(
                "Expected argument to start with '--', got: {argument}"
            )));
        }

        let key = argument.trim_start_matches("--").to_string();

        if key.is_empty() {
            return Err(CliError::InvalidArguments("Empty key after '--'".to_string()));
        }

        let value_string = iterator
            .next()
            .ok_or_else(|| CliError::InvalidArguments(format!("Missing value for argument: {argument}")))?;

        let value = parse_value(value_string);
        inputs.insert(key, value);
    }

    Ok(inputs)
}

fn parse_value(value_string: &str) -> Value {
    serde_json::from_str(value_string).unwrap_or_else(|_| Value::String(value_string.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_string_input() {
        let raw = vec!["--name".to_string(), "Alice".to_string()];
        let result = parse_inputs(raw).unwrap();
        assert_eq!(result.get("name"), Some(&Value::String("Alice".to_string())));
    }

    #[test]
    fn test_parse_number_input() {
        let raw = vec!["--age".to_string(), "25".to_string()];
        let result = parse_inputs(raw).unwrap();
        assert_eq!(result.get("age"), Some(&Value::Number(25.into())));
    }

    #[test]
    fn test_parse_boolean_input() {
        let raw = vec!["--active".to_string(), "true".to_string()];
        let result = parse_inputs(raw).unwrap();
        assert_eq!(result.get("active"), Some(&Value::Bool(true)));
    }

    #[test]
    fn test_parse_array_input() {
        let raw = vec!["--topics".to_string(), r#"["AI", "Rust"]"#.to_string()];
        let result = parse_inputs(raw).unwrap();
        let expected = Value::Array(vec![Value::String("AI".to_string()), Value::String("Rust".to_string())]);
        assert_eq!(result.get("topics"), Some(&expected));
    }

    #[test]
    fn test_parse_multiple_inputs() {
        let raw = vec!["--name".to_string(), "Alice".to_string(), "--age".to_string(), "30".to_string()];
        let result = parse_inputs(raw).unwrap();
        assert_eq!(result.get("name"), Some(&Value::String("Alice".to_string())));
        assert_eq!(result.get("age"), Some(&Value::Number(30.into())));
    }

    #[test]
    fn test_missing_value_error() {
        let raw = vec!["--name".to_string()];
        let result = parse_inputs(raw);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_key_format() {
        let raw = vec!["name".to_string(), "Alice".to_string()];
        let result = parse_inputs(raw);
        assert!(result.is_err());
    }
}
