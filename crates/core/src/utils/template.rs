use anyhow::{Result, anyhow};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

pub fn load_template(path: &str, replacements: &HashMap<String, String>) -> Result<String> {
    // Read the template file
    let content = fs::read_to_string(path)
        .map_err(|e| anyhow!("Failed to read template file '{}': {}", path, e))?;

    // Extract required variables from template
    let required_vars = extract_template_variables(&content);

    // Validate that all required variables are provided
    let provided_vars: HashSet<String> = replacements.keys().cloned().collect();
    validate_template_variables(&required_vars, &provided_vars)?;

    // Perform substitution
    substitute_variables(&content, replacements)
}

fn extract_template_variables(template: &str) -> HashSet<String> {
    let mut vars = HashSet::new();
    let mut chars = template.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' {
            if let Some(&'{') = chars.peek() {
                chars.next(); // consume second {
                let mut var_name = String::new();

                while let Some(c) = chars.next() {
                    if c == '}' {
                        if let Some(&'}') = chars.peek() {
                            chars.next(); // consume second }
                            vars.insert(var_name.trim().to_string());
                            break;
                        }
                    }
                    var_name.push(c);
                }
            }
        }
    }

    vars
}

fn validate_template_variables(required: &HashSet<String>, provided: &HashSet<String>) -> Result<()> {
    // Check for missing variables
    for var in required {
        if !provided.contains(var) {
            return Err(anyhow!("Template variable '{}' is required but not provided", var));
        }
    }

    // Check for unused variables
    for var in provided {
        if !required.contains(var) {
            return Err(anyhow!("Provided variable '{}' is not used in template", var));
        }
    }

    Ok(())
}

fn substitute_variables(template: &str, replacements: &HashMap<String, String>) -> Result<String> {
    let mut result = String::new();
    let mut chars = template.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' {
            if let Some(&'{') = chars.peek() {
                chars.next(); // consume second {
                let mut var_name = String::new();

                while let Some(c) = chars.next() {
                    if c == '}' {
                        if let Some(&'}') = chars.peek() {
                            chars.next(); // consume second }

                            let trimmed = var_name.trim();
                            let value = replacements.get(trimmed)
                                .ok_or_else(|| anyhow!("Variable '{}' not found in replacements", trimmed))?;
                            result.push_str(value);
                            break;
                        }
                    }
                    var_name.push(c);
                }
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_template_variables() {
        let template = "Hello {{ name }}, you are {{ age }} years old!";
        let vars = extract_template_variables(template);

        assert_eq!(vars.len(), 2);
        assert!(vars.contains("name"));
        assert!(vars.contains("age"));
    }

    #[test]
    fn test_substitute_variables() {
        let template = "Hello {{ name }}, you are {{ age }} years old!";
        let mut replacements = HashMap::new();
        replacements.insert("name".to_string(), "John".to_string());
        replacements.insert("age".to_string(), "30".to_string());

        let result = substitute_variables(template, &replacements).unwrap();
        assert_eq!(result, "Hello John, you are 30 years old!");
    }

    #[test]
    fn test_validate_missing_variable() {
        let mut required = HashSet::new();
        required.insert("name".to_string());
        required.insert("age".to_string());

        let mut provided = HashSet::new();
        provided.insert("name".to_string());

        let result = validate_template_variables(&required, &provided);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("age"));
    }

    #[test]
    fn test_validate_unused_variable() {
        let mut required = HashSet::new();
        required.insert("name".to_string());

        let mut provided = HashSet::new();
        provided.insert("name".to_string());
        provided.insert("age".to_string());

        let result = validate_template_variables(&required, &provided);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("age"));
    }
}
