use crate::ast::{Reference, Value};
use crate::execution::error::ExecutionError;
use crate::providers::provider::Message;
use regex::Regex;
use serde_json::Value as JsonValue;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

static INTERPOLATION_PATTERN: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"\{\{([^}]+)\}\}").expect("Invalid regex pattern"));

/// Shared runtime context that can be safely accessed across threads
#[derive(Clone)]
pub struct RuntimeContext {
    inner: Arc<RwLock<RuntimeContextInner>>,
}

struct RuntimeContextInner {
    agent_outputs: HashMap<String, JsonValue>,
    agent_contexts: HashMap<String, Vec<Message>>,
    input_values: HashMap<String, JsonValue>,
}

impl RuntimeContext {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(RuntimeContextInner {
                agent_outputs: HashMap::new(),
                agent_contexts: HashMap::new(),
                input_values: HashMap::new(),
            })),
        }
    }

    pub fn set_agent_output(&self, agent_name: String, output: JsonValue) {
        log::debug!("Setting output for agent: {agent_name}");
        log::trace!("Agent '{agent_name}' output: {output:?}");

        if let Ok(mut inner) = self.inner.write() {
            inner.agent_outputs.insert(agent_name, output);
        } else {
            log::error!("Failed to acquire write lock for RuntimeContext");
        }
    }

    pub fn set_agent_context(&self, agent_name: String, context: Vec<Message>) {
        log::debug!("Setting context for agent: {} ({} messages)", agent_name, context.len());

        if let Ok(mut inner) = self.inner.write() {
            inner.agent_contexts.insert(agent_name, context);
        } else {
            log::error!("Failed to acquire write lock for RuntimeContext");
        }
    }

    pub fn set_input_value(&self, field_name: String, value: JsonValue) {
        log::debug!("Setting input value: {field_name}");
        log::trace!("Input '{field_name}' value: {value:?}");

        if let Ok(mut inner) = self.inner.write() {
            inner.input_values.insert(field_name, value);
        } else {
            log::error!("Failed to acquire write lock for RuntimeContext");
        }
    }

    pub fn resolve_value(&self, value: &Value) -> Result<JsonValue, ExecutionError> {
        match value {
            Value::String(string) => Ok(JsonValue::String(string.clone())),
            Value::MultilineString(string) => Ok(JsonValue::String(string.clone())),
            Value::Number(number) => Ok(JsonValue::Number(
                serde_json::Number::from_f64(*number).unwrap_or(serde_json::Number::from(0)),
            )),
            Value::Boolean(boolean) => Ok(JsonValue::Bool(*boolean)),
            Value::Null => Ok(JsonValue::Null),
            Value::Array(values) => {
                let resolved: Result<Vec<JsonValue>, ExecutionError> =
                    values.iter().map(|v| self.resolve_value(v)).collect();
                Ok(JsonValue::Array(resolved?))
            }
            Value::Object(map) => {
                let mut resolved = serde_json::Map::new();
                for (key, val) in map {
                    resolved.insert(key.clone(), self.resolve_value(val)?);
                }
                Ok(JsonValue::Object(resolved))
            }
            Value::Reference(reference) => self.resolve_reference(reference),
            Value::Interpolated(string) => self.resolve_interpolated_string(string),
            Value::FunctionCall(function_call) => self.resolve_function_call(function_call),
        }
    }

    fn resolve_function_call(&self, function_call: &crate::ast::FunctionCall) -> Result<JsonValue, ExecutionError> {
        match function_call.name.as_str() {
            "file" => {
                let path = function_call
                    .arguments
                    .get("path")
                    .ok_or_else(|| ExecutionError::RuntimeError {
                        agent: "function".to_string(),
                        message: "file function requires 'path' argument".to_string(),
                        suggestion: Some("Provide a file path as the first argument".to_string()),
                    })?;

                let path_str = if let Value::String(string) = path {
                    string.clone()
                } else {
                    return Err(ExecutionError::RuntimeError {
                        agent: "function".to_string(),
                        message: "file path must be a string".to_string(),
                        suggestion: None,
                    });
                };

                let content = std::fs::read_to_string(&path_str).map_err(|error| ExecutionError::RuntimeError {
                    agent: "function".to_string(),
                    message: format!("Failed to read file '{path_str}': {error}"),
                    suggestion: Some("Check that the file exists and is readable".to_string()),
                })?;

                let mut result = content;

                for (key, value) in &function_call.arguments {
                    if key == "path" {
                        continue;
                    }

                    let resolved = self.resolve_value(value)?;
                    let replacement = match resolved {
                        JsonValue::String(string) => string,
                        other => other.to_string(),
                    };

                    let placeholder = format!("{{{{ {key} }}}}");
                    result = result.replace(&placeholder, &replacement);
                }

                Ok(JsonValue::String(result))
            }
            "compact" => Err(ExecutionError::RuntimeError {
                agent: "function".to_string(),
                message: "compact function must be executed asynchronously".to_string(),
                suggestion: Some("compact function is not yet implemented".to_string()),
            }),
            _ => Err(ExecutionError::RuntimeError {
                agent: "function".to_string(),
                message: format!("Unknown function: {}", function_call.name),
                suggestion: Some("Supported functions: file, compact".to_string()),
            }),
        }
    }

    fn resolve_reference(&self, reference: &Reference) -> Result<JsonValue, ExecutionError> {
        log::trace!("Resolving reference: {reference:?}");

        let inner = self.inner.read().map_err(|_| ExecutionError::RuntimeError {
            agent: "context".to_string(),
            message: "Failed to acquire read lock for RuntimeContext".to_string(),
            suggestion: Some("This may indicate a deadlock or poisoned lock".to_string()),
        })?;

        match reference {
            Reference::Agent { agent, field } => {
                if let Some(output) = inner.agent_outputs.get(agent) {
                    if field == "_output" {
                        log::trace!("Resolved agent '{agent}' full output");
                        return Ok(output.clone());
                    }

                    if let JsonValue::Object(map) = output {
                        if let Some(value) = map.get(field) {
                            log::trace!("Resolved agent '{agent}' field '{field}'");
                            return Ok(value.clone());
                        }
                    }

                    log::warn!("Field '{field}' not found in agent '{agent}' output");
                    return Err(ExecutionError::RuntimeError {
                        agent: agent.clone(),
                        message: format!("Field '{field}' not found in agent output"),
                        suggestion: Some("Check that the agent produces this field".to_string()),
                    });
                }

                log::warn!("Agent '{agent}' output not found");
                Err(ExecutionError::RuntimeError {
                    agent: agent.clone(),
                    message: format!("Agent '{agent}' output not found"),
                    suggestion: Some("Check that the agent has executed".to_string()),
                })
            }
            Reference::AgentOutput { agent } => {
                if let Some(output) = inner.agent_outputs.get(agent) {
                    log::trace!("Resolved agent '{agent}' output");

                    if let JsonValue::Object(map) = output {
                        if map.len() == 1 {
                            if let Some((_key, value)) = map.iter().next() {
                                log::trace!("Auto-unwrapping single-field object for agent '{agent}'");
                                return Ok(value.clone());
                            }
                        }
                    }

                    return Ok(output.clone());
                }

                log::warn!("Agent '{agent}' output not found");
                Err(ExecutionError::RuntimeError {
                    agent: agent.clone(),
                    message: format!("Agent '{agent}' output not found"),
                    suggestion: Some("Check that the agent has executed".to_string()),
                })
            }
            Reference::AgentContext { agent } => {
                if let Some(context) = inner.agent_contexts.get(agent) {
                    log::trace!("Resolved agent '{}' context ({} messages)", agent, context.len());
                    return Ok(serde_json::to_value(context).unwrap_or(JsonValue::Null));
                }

                log::warn!("Agent '{agent}' context not found");
                Err(ExecutionError::RuntimeError {
                    agent: agent.clone(),
                    message: "Agent context not found".to_string(),
                    suggestion: Some("Check that the agent has executed".to_string()),
                })
            }
            Reference::Input { field } => {
                if let Some(value) = inner.input_values.get(field) {
                    log::trace!("Resolved input field '{field}'");
                    return Ok(value.clone());
                }

                log::warn!("Input field '{field}' not found");
                Err(ExecutionError::RuntimeError {
                    agent: "input".to_string(),
                    message: format!("Input field '{field}' not found"),
                    suggestion: Some("Provide this input value when executing the workflow".to_string()),
                })
            }
            Reference::Schema { name } => {
                log::trace!("Resolved schema reference: {name}");
                Ok(JsonValue::String(format!("schema:{name}")))
            }
            Reference::Tool { name } => {
                log::trace!("Resolved tool reference: {name}");
                Ok(JsonValue::String(format!("tool.{name}")))
            }
        }
    }

    fn resolve_interpolated_string(&self, template: &str) -> Result<JsonValue, ExecutionError> {
        let mut result = Cow::Borrowed(template);

        for capture in INTERPOLATION_PATTERN.captures_iter(template) {
            let full_match = &capture[0];
            let reference_text = capture[1].trim();

            let reference = self.parse_reference_from_string(reference_text)?;
            let resolved = self.resolve_reference(&reference)?;

            let replacement = match resolved {
                JsonValue::String(string) => string,
                other => other.to_string(),
            };

            result = Cow::Owned(result.replace(full_match, &replacement));
        }

        Ok(JsonValue::String(result.into_owned()))
    }

    fn parse_reference_from_string(&self, text: &str) -> Result<Reference, ExecutionError> {
        let parts: Vec<&str> = text.split('.').collect();

        if parts.len() == 2 {
            if parts[0] == "input" {
                return Ok(Reference::Input {
                    field: parts[1].to_string(),
                });
            } else if parts[0] == "schema" {
                return Ok(Reference::Schema {
                    name: parts[1].to_string(),
                });
            } else if parts[0] == "agent" {
                return Ok(Reference::AgentOutput {
                    agent: parts[1].to_string(),
                });
            }
        } else if parts.len() == 3 {
            if parts[0] == "agent" {
                if parts[2] == "context" {
                    return Ok(Reference::AgentContext {
                        agent: parts[1].to_string(),
                    });
                }
                return Ok(Reference::Agent {
                    agent: parts[1].to_string(),
                    field: parts[2].to_string(),
                });
            }
        } else if parts.len() == 4 && parts[0] == "agent" && parts[2] == "context" {
            return Ok(Reference::AgentContext {
                agent: parts[1].to_string(),
            });
        }

        Err(ExecutionError::RuntimeError {
            agent: "parser".to_string(),
            message: format!("Invalid reference: {text}"),
            suggestion: Some(
                "Use format 'agent.name', 'agent.name.field', 'input.field', or 'schema.name'".to_string(),
            ),
        })
    }
}

impl Default for RuntimeContext {
    fn default() -> Self {
        Self::new()
    }
}
