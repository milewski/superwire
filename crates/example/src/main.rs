use engine_ai_core::execution::ExecutionEngine;
use engine_ai_core::parser::AstBuilder;
use serde_json::Value;
use std::collections::HashMap;
use std::env;

#[tokio::main]
async fn main() {
    colog::init();

    let arguments: Vec<String> = env::args().collect();

    if arguments.len() < 2 {
        eprintln!("Usage: {} <workflow.ai> [VALUES...] [OPTIONS]", arguments[0]);
        eprintln!();
        eprintln!("Positional arguments:");
        eprintln!("  VALUES...               Input values in the order defined in the workflow's input block");
        eprintln!();
        eprintln!("Options:");
        eprintln!("  --input <json_file>     Load inputs from a JSON file");
        eprintln!("  --value <key>=<value>   Set an input value (can be used multiple times)");
        eprintln!();
        eprintln!("Examples:");
        eprintln!(
            "  {} workflow.ai Alice                    # Positional argument",
            arguments[0]
        );
        eprintln!(
            "  {} workflow.ai Alice 30                 # Multiple positional arguments",
            arguments[0]
        );
        eprintln!(
            "  {} workflow.ai --input inputs.json      # From JSON file",
            arguments[0]
        );
        eprintln!(
            "  {} workflow.ai --value name=Alice       # Named argument",
            arguments[0]
        );
        eprintln!(
            "  {} workflow.ai Alice --value age=30     # Mix positional and named",
            arguments[0]
        );
        std::process::exit(1);
    }

    let workflow_path = &arguments[1];

    // Parse the workflow to get input field names
    let workflow_content = std::fs::read_to_string(workflow_path).unwrap_or_else(|error| {
        eprintln!("Failed to read workflow file '{}': {}", workflow_path, error);
        std::process::exit(1);
    });

    let builder = AstBuilder::new(workflow_path.to_string());
    let workflow = builder.parse(&workflow_content).unwrap_or_else(|error| {
        eprintln!("Failed to parse workflow: {}", error);
        std::process::exit(1);
    });

    let input_field_names: Vec<String> = if let Some(input_block) = &workflow.input {
        input_block.fields.iter().map(|f| f.name.clone()).collect()
    } else {
        Vec::new()
    };

    let mut inputs = HashMap::new();
    let mut positional_values = Vec::new();

    let mut index = 2;
    while index < arguments.len() {
        let arg = &arguments[index];

        if arg == "--input" {
            if index + 1 >= arguments.len() {
                eprintln!("Error: --input requires a file path");
                std::process::exit(1);
            }

            let input_file = &arguments[index + 1];
            let input_content = std::fs::read_to_string(input_file).unwrap_or_else(|error| {
                eprintln!("Failed to read input file '{}': {}", input_file, error);
                std::process::exit(1);
            });

            let input_json: Value = serde_json::from_str(&input_content).unwrap_or_else(|error| {
                eprintln!("Failed to parse input JSON: {}", error);
                std::process::exit(1);
            });

            if let Value::Object(map) = input_json {
                inputs.extend(map.into_iter());
            } else {
                eprintln!("Input JSON must be an object");
                std::process::exit(1);
            }

            index += 2;
        } else if arg == "--value" {
            if index + 1 >= arguments.len() {
                eprintln!("Error: --value requires a key=value pair");
                std::process::exit(1);
            }

            let value_arg = &arguments[index + 1];

            if let Some((key, value)) = value_arg.split_once('=') {
                let parsed_value = parse_value(value);
                inputs.insert(key.to_string(), parsed_value);
            } else {
                eprintln!("Error: --value argument must be in format key=value");
                eprintln!("Got: {}", value_arg);
                std::process::exit(1);
            }

            index += 2;
        } else if let Some(field_name) = arg.strip_prefix("--") {
            // Treat as --field_name value
            // Remove "--" prefix

            if index + 1 >= arguments.len() {
                eprintln!("Error: {} requires a value", arg);
                std::process::exit(1);
            }

            let value = &arguments[index + 1];
            inputs.insert(field_name.to_string(), parse_value(value));

            index += 2;
        } else {
            // Positional argument
            positional_values.push(arg.clone());
            index += 1;
        }
    }

    // Map positional values to input field names
    for (i, value) in positional_values.iter().enumerate() {
        if i < input_field_names.len() {
            let field_name = &input_field_names[i];
            // Only set if not already set by --value or --input
            if !inputs.contains_key(field_name) {
                inputs.insert(field_name.clone(), parse_value(value));
            }
        } else {
            eprintln!(
                "Warning: Extra positional argument '{}' ignored (workflow only has {} input fields)",
                value,
                input_field_names.len()
            );
        }
    }

    let engine = ExecutionEngine::new();

    match engine.execute_workflow_with_inputs(workflow_path, inputs).await {
        Ok(output) => {
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        Err(error) => {
            eprintln!("Error: {}", error);
            std::process::exit(1);
        }
    }
}

fn parse_value(value: &str) -> Value {
    if value.is_empty() {
        return Value::String(value.to_string());
    }

    if value == "true" {
        return Value::Bool(true);
    }

    if value == "false" {
        return Value::Bool(false);
    }

    if value == "null" {
        return Value::Null;
    }

    if let Ok(number) = value.parse::<i64>() {
        return Value::Number(number.into());
    }

    if let Ok(number) = value.parse::<f64>() {
        if let Some(num) = serde_json::Number::from_f64(number) {
            return Value::Number(num);
        }
    }

    if value.starts_with('[') && value.ends_with(']') {
        if let Ok(array) = serde_json::from_str::<Value>(value) {
            return array;
        }
    }

    if value.starts_with('{') && value.ends_with('}') {
        if let Ok(object) = serde_json::from_str::<Value>(value) {
            return object;
        }
    }

    Value::String(value.to_string())
}
