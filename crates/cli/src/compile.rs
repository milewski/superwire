use crate::error::CliError;
use engine_ai_core::ast::{Value, Workflow};
use engine_ai_core::parser::builder::AstBuilder;
use engine_ai_core::validation::validator::WorkflowValidator;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use uuid::Uuid;

pub fn compile_workflow(workflow_path: &Path, output_path: &Path) -> Result<(), CliError> {
    log::info!("Parsing workflow...");
    let source = fs::read_to_string(workflow_path)?;
    let builder = AstBuilder::new(workflow_path.display().to_string());
    let workflow = builder.parse(&source)?;

    log::info!("Validating workflow...");
    WorkflowValidator::validate(&workflow).map_err(|errors| {
        CliError::CompilationError(format!(
            "Validation failed: {}",
            errors
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })?;

    log::info!("Embedding files...");
    let embedded_files = embed_files(&workflow, workflow_path)?;

    log::info!("Serializing AST...");
    let serialized_ast = bincode::serialize(&workflow)?;

    log::info!("Generating executable source...");
    let source_code = generate_executable_source(&serialized_ast, &embedded_files, &workflow)?;

    log::info!("Building executable...");
    build_executable(&source_code, output_path)?;

    Ok(())
}

fn embed_files(workflow: &Workflow, workflow_path: &Path) -> Result<HashMap<String, Vec<u8>>, CliError> {
    let mut embedded_files = HashMap::new();
    let workflow_dir = workflow_path
        .parent()
        .ok_or_else(|| CliError::CompilationError("Invalid workflow path".to_string()))?;

    collect_file_references(&workflow.agents, workflow_dir, &mut embedded_files)?;

    if let Some(output_block) = &workflow.output {
        for field in &output_block.fields {
            collect_value_file_references(&field.value, workflow_dir, &mut embedded_files)?;
        }
    }

    Ok(embedded_files)
}

fn collect_file_references(
    agents: &[engine_ai_core::ast::Agent],
    workflow_dir: &Path,
    embedded_files: &mut HashMap<String, Vec<u8>>,
) -> Result<(), CliError> {
    for agent in agents {
        for property in &agent.properties {
            match property {
                engine_ai_core::ast::AgentProperty::Context { value, .. }
                | engine_ai_core::ast::AgentProperty::Tools { value, .. }
                | engine_ai_core::ast::AgentProperty::Model { value, .. } => {
                    collect_value_file_references(value, workflow_dir, embedded_files)?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn collect_value_file_references(
    value: &Value,
    workflow_dir: &Path,
    embedded_files: &mut HashMap<String, Vec<u8>>,
) -> Result<(), CliError> {
    match value {
        Value::FunctionCall(call) if call.name == "file" => {
            if let Some(Value::String(file_path)) = call.arguments.get("path") {
                let full_path = workflow_dir.join(file_path);
                if !embedded_files.contains_key(file_path) {
                    let content = fs::read(&full_path).map_err(|error| {
                        CliError::CompilationError(format!("Failed to read file {}: {}", full_path.display(), error))
                    })?;
                    embedded_files.insert(file_path.clone(), content);
                    log::debug!("Embedded file: {file_path}");
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_value_file_references(item, workflow_dir, embedded_files)?;
            }
        }
        Value::Object(fields) => {
            for field_value in fields.values() {
                collect_value_file_references(field_value, workflow_dir, embedded_files)?;
            }
        }
        Value::FunctionCall(call) => {
            for argument in call.arguments.values() {
                collect_value_file_references(argument, workflow_dir, embedded_files)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn generate_executable_source(
    serialized_ast: &[u8],
    embedded_files: &HashMap<String, Vec<u8>>,
    workflow: &Workflow,
) -> Result<String, CliError> {
    let ast_bytes = format_byte_array(serialized_ast);
    let files_data = generate_embedded_files_code(embedded_files);
    let cli_parser = generate_cli_parser(workflow)?;

    Ok(format!(
        r#"
{cli_parser}

#[tokio::main]
async fn main() {{
    env_logger::init();

    if let Err(error) = run().await {{
        eprintln!("Error: {{error}}");
        std::process::exit(1);
    }}
}}

async fn run() -> Result<(), Box<dyn std::error::Error>> {{
    let inputs = parse_args();

    let workflow: engine_ai_core::ast::Workflow = bincode::deserialize(WORKFLOW_AST)?;

    let engine = engine_ai_core::execution::engine::ExecutionEngine::new();
    let result = engine.execute_parsed_workflow_with_inputs(&workflow, inputs).await?;

    let output = serde_json::to_string_pretty(&result)?;
    println!("{{output}}");

    Ok(())
}}

const WORKFLOW_AST: &[u8] = &{ast_bytes};

{files_data}
"#
    ))
}

fn format_byte_array(bytes: &[u8]) -> String {
    let byte_strings: Vec<String> = bytes.iter().map(|byte| format!("{byte}")).collect();
    format!("[{}]", byte_strings.join(", "))
}

fn generate_embedded_files_code(embedded_files: &HashMap<String, Vec<u8>>) -> String {
    if embedded_files.is_empty() {
        return String::from("const EMBEDDED_FILES: &[(&str, &[u8])] = &[];");
    }

    let mut entries = Vec::new();
    for (path, content) in embedded_files {
        let bytes = format_byte_array(content);
        entries.push(format!("    (\"{path}\", &{bytes})"));
    }

    format!(
        "const EMBEDDED_FILES: &[(&str, &[u8])] = &[\n{}\n];",
        entries.join(",\n")
    )
}

fn generate_cli_parser(workflow: &Workflow) -> Result<String, CliError> {
    let mut arg_definitions = Vec::new();
    let mut arg_parsers = Vec::new();

    if let Some(input_block) = &workflow.input {
        for field in &input_block.fields {
            let field_name = &field.name;
            let _arg_name = field_name.replace('_', "-");

            arg_definitions.push(format!(
                "        #[arg(long, help = \"Input field: {field_name}\")]\n        {field_name}: Option<String>,"
            ));

            arg_parsers.push(format!(
                r#"    if let Some(value) = args.{field_name} {{
        inputs.insert("{field_name}".to_string(), parse_value(&value));
    }}"#
            ));
        }
    }

    let args_struct = if arg_definitions.is_empty() {
        String::from(
            r"#[derive(clap::Parser)]
struct Args {}",
        )
    } else {
        format!(
            r"#[derive(clap::Parser)]
struct Args {{
{}
}}",
            arg_definitions.join("\n")
        )
    };

    let parser_body = if arg_parsers.is_empty() {
        String::from(
            r"fn parse_args() -> std::collections::HashMap<String, serde_json::Value> {
    use clap::Parser;
    let _args = Args::parse();
    std::collections::HashMap::new()
}",
        )
    } else {
        format!(
            r"fn parse_args() -> std::collections::HashMap<String, serde_json::Value> {{
    use clap::Parser;
    let args = Args::parse();
    let mut inputs = std::collections::HashMap::new();

{}

    inputs
}}

fn parse_value(value: &str) -> serde_json::Value {{
    serde_json::from_str(value).unwrap_or_else(|_| serde_json::Value::String(value.to_string()))
}}",
            arg_parsers.join("\n")
        )
    };

    Ok(format!("{args_struct}\n\n{parser_body}"))
}

fn build_executable(source_code: &str, output_path: &Path) -> Result<(), CliError> {
    let temp_dir = std::env::temp_dir().join(format!("engine-ai-build-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_dir)?;

    let src_dir = temp_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    let main_rs = src_dir.join("main.rs");
    fs::write(&main_rs, source_code)?;

    let core_path = std::env::current_dir()?
        .join("crates/core")
        .canonicalize()
        .map_err(|error| CliError::CompilationError(format!("Failed to find engine-ai-core: {error}")))?;

    let cargo_toml = temp_dir.join("Cargo.toml");
    let cargo_toml_content = format!(
        r#"[package]
name = "compiled-workflow"
version = "0.1.0"
edition = "2021"

[dependencies]
engine-ai-core = {{ path = "{}" }}
tokio = {{ version = "1", features = ["full"] }}
serde_json = "1"
bincode = "1.3"
clap = {{ version = "4.5", features = ["derive"] }}
env_logger = "0.11"

[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
strip = true
"#,
        core_path.display()
    );
    fs::write(&cargo_toml, cargo_toml_content)?;

    log::info!("Running cargo build...");
    let output = std::process::Command::new("cargo")
        .arg("build")
        .arg("--release")
        .arg("--manifest-path")
        .arg(&cargo_toml)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CliError::CompilationError(format!("Cargo build failed: {stderr}")));
    }

    let binary_name = if cfg!(windows) {
        "compiled-workflow.exe"
    } else {
        "compiled-workflow"
    };
    let compiled_binary = temp_dir.join("target/release").join(binary_name);

    fs::copy(&compiled_binary, output_path)?;

    fs::remove_dir_all(&temp_dir)?;

    Ok(())
}
