use crate::runtime::error::WorkflowRuntimeError;
use async_trait::async_trait;
use engine_ai_agent::{
    AgentConfig, Context, OllamaProvider, OpenAIProvider, Provider, ProviderError, ProviderResponse, StopReason, ToolCall, ToolDefinition,
};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::Arc;

pub trait WorkflowProviderFactory: Send + Sync {
    fn build_provider(
        &self,
        agent_name: &str,
        provider_name: &str,
        provider_settings: &Map<String, Value>,
        model_name: &str,
    ) -> Result<DynamicProvider, WorkflowRuntimeError>;
}

#[derive(Clone)]
pub struct DynamicProvider {
    inner_provider: Arc<dyn Provider + Send + Sync>,
}

impl DynamicProvider {
    #[must_use]
    pub fn new<ProviderType>(provider: ProviderType) -> Self
    where
        ProviderType: Provider + Send + Sync + 'static,
    {
        Self {
            inner_provider: Arc::new(provider),
        }
    }
}

#[async_trait]
impl Provider for DynamicProvider {
    async fn generate(&self, context: &Context, tools: &[ToolDefinition], config: &AgentConfig) -> Result<ProviderResponse, ProviderError> {
        self.inner_provider.generate(context, tools, config).await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderDriver {
    OpenAi,
    Ollama,
}

impl ProviderDriver {
    fn from_driver_name(driver_name: &str) -> Option<Self> {
        match driver_name {
            "openai" => Some(Self::OpenAi),
            "ollama" => Some(Self::Ollama),
            _ => None,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultProviderFactory;

impl WorkflowProviderFactory for DefaultProviderFactory {
    fn build_provider(
        &self,
        _agent_name: &str,
        provider_name: &str,
        provider_settings: &Map<String, Value>,
        model_name: &str,
    ) -> Result<DynamicProvider, WorkflowRuntimeError> {
        let driver_name = string_setting(provider_settings, "driver").ok_or_else(|| WorkflowRuntimeError::ProviderFactoryFailed {
            message: format!("provider '{provider_name}' is missing a string `driver` setting"),
        })?;

        let provider_driver =
            ProviderDriver::from_driver_name(driver_name.as_str()).ok_or_else(|| WorkflowRuntimeError::ProviderFactoryFailed {
                message: format!("provider '{provider_name}' has unknown driver '{driver_name}'. Supported drivers: openai, ollama"),
            })?;

        match provider_driver {
            ProviderDriver::OpenAi => {
                let openai_api_key = string_setting(provider_settings, "api_key").or_else(|| std::env::var("OPENAI_API_KEY").ok());
                let openai_base_url =
                    string_setting(provider_settings, "base_url").or_else(|| string_setting(provider_settings, "api_endpoint"));

                let openai_provider = match (openai_base_url, openai_api_key) {
                    (Some(base_url), Some(api_key)) => OpenAIProvider::new_with_base_url(base_url, api_key, model_name),
                    (Some(base_url), None) => OpenAIProvider::new_local(base_url, model_name),
                    (None, Some(api_key)) => OpenAIProvider::new(api_key, model_name),
                    (None, None) => {
                        return Err(WorkflowRuntimeError::ProviderFactoryFailed {
                            message: format!(
                                "provider '{provider_name}' with openai driver requires `api_key`, `base_url`, `api_endpoint`, or OPENAI_API_KEY"
                            ),
                        });
                    }
                };

                Ok(DynamicProvider::new(openai_provider))
            }
            ProviderDriver::Ollama => {
                let endpoint_setting = string_setting(provider_settings, "api_endpoint")
                    .or_else(|| string_setting(provider_settings, "base_url"))
                    .or_else(|| string_setting(provider_settings, "host"));

                let (host, port) = if let Some(endpoint_value) = endpoint_setting {
                    parse_ollama_endpoint(endpoint_value.as_str())?
                } else {
                    ("http://localhost".to_owned(), 11434)
                };

                let ollama_provider = OllamaProvider::new(host, port, model_name.to_owned());

                Ok(DynamicProvider::new(ollama_provider))
            }
        }
    }
}

fn string_setting(provider_settings: &Map<String, Value>, setting_name: &str) -> Option<String> {
    provider_settings.get(setting_name).and_then(Value::as_str).map(str::to_owned)
}

fn parse_ollama_endpoint(endpoint_value: &str) -> Result<(String, u16), WorkflowRuntimeError> {
    let trimmed_endpoint = endpoint_value.trim();

    if trimmed_endpoint.is_empty() {
        return Err(WorkflowRuntimeError::ProviderFactoryFailed {
            message: "ollama endpoint cannot be empty".to_owned(),
        });
    }

    let (scheme, endpoint_without_scheme) = if let Some(stripped_endpoint) = trimmed_endpoint.strip_prefix("http://") {
        ("http", stripped_endpoint)
    } else if let Some(stripped_endpoint) = trimmed_endpoint.strip_prefix("https://") {
        ("https", stripped_endpoint)
    } else {
        ("http", trimmed_endpoint)
    };

    let host_and_port = endpoint_without_scheme
        .split('/')
        .next()
        .ok_or_else(|| WorkflowRuntimeError::ProviderFactoryFailed {
            message: format!("invalid ollama endpoint '{endpoint_value}'"),
        })?;

    if host_and_port.is_empty() {
        return Err(WorkflowRuntimeError::ProviderFactoryFailed {
            message: format!("invalid ollama endpoint '{endpoint_value}'"),
        });
    }

    let mut host_and_port_segments = host_and_port.splitn(2, ':');
    let host_segment = host_and_port_segments.next().unwrap_or_default();

    if host_segment.is_empty() {
        return Err(WorkflowRuntimeError::ProviderFactoryFailed {
            message: format!("invalid ollama endpoint '{endpoint_value}'"),
        });
    }

    let port_segment = host_and_port_segments.next();

    let port = if let Some(port_segment) = port_segment {
        port_segment
            .parse::<u16>()
            .map_err(|_| WorkflowRuntimeError::ProviderFactoryFailed {
                message: format!("invalid ollama port in endpoint '{endpoint_value}'"),
            })?
    } else {
        11434
    };

    let host = format!("{scheme}://{host_segment}");

    Ok((host, port))
}

#[derive(Debug, Clone, Default)]
pub struct ScriptedProviderFactory {
    outputs_by_agent_name: HashMap<String, Value>,
}

impl ScriptedProviderFactory {
    #[must_use]
    pub fn new(outputs_by_agent_name: HashMap<String, Value>) -> Self {
        Self { outputs_by_agent_name }
    }
}

impl WorkflowProviderFactory for ScriptedProviderFactory {
    fn build_provider(
        &self,
        agent_name: &str,
        _provider_name: &str,
        _provider_settings: &Map<String, Value>,
        _model_name: &str,
    ) -> Result<DynamicProvider, WorkflowRuntimeError> {
        let output_value =
            self.outputs_by_agent_name
                .get(agent_name)
                .cloned()
                .ok_or_else(|| WorkflowRuntimeError::ProviderFactoryFailed {
                    message: format!("scripted output is missing for agent '{agent_name}'"),
                })?;

        Ok(DynamicProvider::new(ScriptedProvider::new(output_value)))
    }
}

#[derive(Debug, Clone)]
struct ScriptedProvider {
    output_value: Value,
}

impl ScriptedProvider {
    fn new(output_value: Value) -> Self {
        Self { output_value }
    }
}

#[async_trait]
impl Provider for ScriptedProvider {
    async fn generate(
        &self,
        _context: &Context,
        _tools: &[ToolDefinition],
        _config: &AgentConfig,
    ) -> Result<ProviderResponse, ProviderError> {
        let finalize_tool_call = ToolCall {
            id: "scripted-finalize".to_owned(),
            name: "finalize".to_owned(),
            arguments: serde_json::json!({
                "output": {
                    "type": "success",
                    "answer": self.output_value.clone(),
                }
            }),
        };

        Ok(ProviderResponse {
            tool_calls: vec![finalize_tool_call],
            text: None,
            stop_reason: StopReason::ToolCalls,
            usage: None,
        })
    }
}
