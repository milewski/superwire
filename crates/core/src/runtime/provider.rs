use crate::dsl::{Declaration, Expression, ObjectField, ProviderDeclaration, Workflow};
use crate::runtime::error::WorkflowRuntimeError;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderDriver {
    OpenAI,
    Ollama,
}

impl ProviderDriver {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAI => "openai",
            Self::Ollama => "ollama",
        }
    }

    #[must_use]
    pub fn default_endpoint(self) -> &'static str {
        match self {
            Self::OpenAI => "https://api.openai.com/v1",
            Self::Ollama => "http://127.0.0.1:11434",
        }
    }

    fn parse(driver_name: &str) -> Option<Self> {
        match driver_name {
            "openai" => Some(Self::OpenAI),
            "ollama" => Some(Self::Ollama),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderConfig {
    OpenAI(OpenAIProviderConfig),
    Ollama(OllamaProviderConfig),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAIProviderConfig {
    pub driver: ProviderDriver,
    pub api_endpoint: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OllamaProviderConfig {
    pub driver: ProviderDriver,
    pub endpoint: String,
    pub host: String,
    pub port: u16,
}

pub fn build_provider_index(workflow: &Workflow) -> Result<HashMap<String, ProviderConfig>, WorkflowRuntimeError> {
    let mut provider_index = HashMap::new();

    for declaration in workflow.declarations() {
        let Declaration::Provider(provider_declaration) = declaration else {
            continue;
        };

        let provider_config = parse_provider_config(provider_declaration)?;
        provider_index.insert(provider_declaration.name.clone(), provider_config);
    }

    Ok(provider_index)
}

fn parse_provider_config(provider_declaration: &ProviderDeclaration) -> Result<ProviderConfig, WorkflowRuntimeError> {
    let Some(driver) = required_string_property(provider_declaration, "driver")? else {
        return Err(WorkflowRuntimeError::ProviderConfiguration {
            provider_name: provider_declaration.name.clone(),
            message: "missing `driver` property".to_string(),
        });
    };

    let Some(provider_driver) = ProviderDriver::parse(&driver) else {
        return Err(WorkflowRuntimeError::ProviderConfiguration {
            provider_name: provider_declaration.name.clone(),
            message: format!("unsupported provider driver `{driver}`"),
        });
    };

    match provider_driver {
        ProviderDriver::OpenAI => parse_openai_config(provider_declaration, provider_driver),
        ProviderDriver::Ollama => parse_ollama_config(provider_declaration, provider_driver),
    }
}

fn parse_openai_config(
    provider_declaration: &ProviderDeclaration,
    provider_driver: ProviderDriver,
) -> Result<ProviderConfig, WorkflowRuntimeError> {
    let api_endpoint = optional_string_property(provider_declaration, "api_endpoint")?
        .or(optional_string_property(provider_declaration, "endpoint")?)
        .unwrap_or_else(|| provider_driver.default_endpoint().to_string());

    let api_key = optional_string_property(provider_declaration, "api_key")?;

    Ok(ProviderConfig::OpenAI(OpenAIProviderConfig {
        driver: provider_driver,
        api_endpoint,
        api_key,
    }))
}

fn parse_ollama_config(
    provider_declaration: &ProviderDeclaration,
    provider_driver: ProviderDriver,
) -> Result<ProviderConfig, WorkflowRuntimeError> {
    let endpoint = optional_string_property(provider_declaration, "api_endpoint")?
        .or(optional_string_property(provider_declaration, "endpoint")?)
        .unwrap_or_else(|| provider_driver.default_endpoint().to_string());

    let parsed_endpoint = reqwest::Url::parse(&endpoint).map_err(|error| WorkflowRuntimeError::ProviderConfiguration {
        provider_name: provider_declaration.name.clone(),
        message: format!("invalid ollama endpoint `{endpoint}`: {error}"),
    })?;

    let Some(host_name) = parsed_endpoint.host_str() else {
        return Err(WorkflowRuntimeError::ProviderConfiguration {
            provider_name: provider_declaration.name.clone(),
            message: format!("ollama endpoint is missing host: `{endpoint}`"),
        });
    };

    let Some(port) = parsed_endpoint.port_or_known_default() else {
        return Err(WorkflowRuntimeError::ProviderConfiguration {
            provider_name: provider_declaration.name.clone(),
            message: format!("ollama endpoint is missing port: `{endpoint}`"),
        });
    };

    let host = format!("{}://{host_name}", parsed_endpoint.scheme());

    Ok(ProviderConfig::Ollama(OllamaProviderConfig {
        driver: provider_driver,
        endpoint,
        host,
        port,
    }))
}

fn required_string_property(
    provider_declaration: &ProviderDeclaration,
    property_name: &str,
) -> Result<Option<String>, WorkflowRuntimeError> {
    optional_string_property(provider_declaration, property_name)
}

fn optional_string_property(
    provider_declaration: &ProviderDeclaration,
    property_name: &str,
) -> Result<Option<String>, WorkflowRuntimeError> {
    let Some(property_value) = provider_property_by_name(provider_declaration.properties.as_slice(), property_name) else {
        return Ok(None);
    };

    let Expression::StringLiteral(string_literal) = &property_value.value else {
        return Err(WorkflowRuntimeError::ProviderConfiguration {
            provider_name: provider_declaration.name.clone(),
            message: format!("`{property_name}` must be a string literal"),
        });
    };

    Ok(Some(string_literal.clone()))
}

fn provider_property_by_name<'property>(
    provider_properties: &'property [ObjectField],
    property_name: &str,
) -> Option<&'property ObjectField> {
    provider_properties
        .iter()
        .find(|provider_property| provider_property.name == property_name)
}

#[cfg(test)]
mod tests {
    use super::{build_provider_index, ProviderConfig, ProviderDriver};

    #[test]
    fn uses_openai_default_endpoint_when_not_provided() {
        let workflow = crate::parse_inline_workflow! {
            provider openai {
                driver: "openai"
                models: ["model-a"]
            }
        };

        let provider_index = build_provider_index(&workflow).expect("provider index should build");
        let provider = provider_index.get("openai").expect("openai provider should exist");

        match provider {
            ProviderConfig::OpenAI(openai_provider_config) => {
                assert_eq!(openai_provider_config.driver, ProviderDriver::OpenAI);
                assert_eq!(openai_provider_config.api_endpoint, ProviderDriver::OpenAI.default_endpoint());
            }
            ProviderConfig::Ollama(_) => panic!("expected openai provider config"),
        }
    }

    #[test]
    fn uses_ollama_default_endpoint_when_not_provided() {
        let workflow = crate::parse_inline_workflow! {
            provider ollama {
                driver: "ollama"
                models: ["model-a"]
            }
        };

        let provider_index = build_provider_index(&workflow).expect("provider index should build");
        let provider = provider_index.get("ollama").expect("ollama provider should exist");

        match provider {
            ProviderConfig::OpenAI(_) => panic!("expected ollama provider config"),
            ProviderConfig::Ollama(ollama_provider_config) => {
                assert_eq!(ollama_provider_config.driver, ProviderDriver::Ollama);
                assert_eq!(ollama_provider_config.endpoint, ProviderDriver::Ollama.default_endpoint());
                assert_eq!(ollama_provider_config.host, "http://127.0.0.1");
                assert_eq!(ollama_provider_config.port, 11434);
            }
        }
    }
}
