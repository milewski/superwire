use crate::dsl::{Declaration, Expression, ObjectField, ProviderDeclaration, Workflow};
use crate::runtime::error::WorkflowRuntimeError;
use std::collections::HashMap;

mod ollama;
mod openai;

pub use ollama::OllamaProviderConfig;
pub use openai::OpenAIProviderConfig;

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
    pub fn parse(driver_name: &str) -> Option<Self> {
        match driver_name {
            "openai" => Some(Self::OpenAI),
            "ollama" => Some(Self::Ollama),
            _ => None,
        }
    }
}

pub trait ProviderConfigParser {
    fn parse(provider_declaration: &ProviderDeclaration) -> Result<ProviderConfig, WorkflowRuntimeError>;

    fn required_string_property(provider_declaration: &ProviderDeclaration, property_name: &str) -> Result<String, WorkflowRuntimeError> {
        let Some(property_value) = Self::provider_property_by_name(provider_declaration.properties.as_slice(), property_name) else {
            return Err(WorkflowRuntimeError::ProviderConfiguration {
                provider_name: provider_declaration.name.clone(),
                message: format!("missing `{property_name}` property"),
            });
        };

        let Expression::StringLiteral(string_literal) = &property_value.value else {
            return Err(WorkflowRuntimeError::ProviderConfiguration {
                provider_name: provider_declaration.name.clone(),
                message: format!("`{property_name}` must be a string literal"),
            });
        };

        Ok(string_literal.clone())
    }

    fn provider_property_by_name<'property>(
        provider_properties: &'property [ObjectField],
        property_name: &str,
    ) -> Option<&'property ObjectField> {
        provider_properties
            .iter()
            .find(|provider_property| provider_property.name == property_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderConfig {
    OpenAI(OpenAIProviderConfig),
    Ollama(OllamaProviderConfig),
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
    let driver_name =
        <openai::OpenAIProviderConfigParser as ProviderConfigParser>::required_string_property(provider_declaration, "driver")?;

    let Some(provider_driver) = ProviderDriver::parse(&driver_name) else {
        return Err(WorkflowRuntimeError::ProviderConfiguration {
            provider_name: provider_declaration.name.clone(),
            message: format!("unsupported provider driver `{driver_name}`"),
        });
    };

    match provider_driver {
        ProviderDriver::OpenAI => openai::OpenAIProviderConfigParser::parse(provider_declaration),
        ProviderDriver::Ollama => ollama::OllamaProviderConfigParser::parse(provider_declaration),
    }
}

#[cfg(test)]
mod tests {
    use super::{build_provider_index, ProviderConfig};
    use crate::runtime::error::WorkflowRuntimeError;

    #[test]
    fn parses_openai_provider_with_required_endpoint_and_api_key() {
        let workflow = crate::parse_inline_workflow! {
            provider openai {
                driver: "openai"
                endpoint: "https://api.openai.com/v1"
                api_key: "test-api-key"
                models: ["model-a"]
            }
        };

        let provider_index = build_provider_index(&workflow).expect("provider index should build");
        let provider = provider_index.get("openai").expect("openai provider should exist");

        match provider {
            ProviderConfig::OpenAI(openai_provider_config) => {
                assert_eq!(openai_provider_config.endpoint, "https://api.openai.com/v1");
                assert_eq!(openai_provider_config.api_key, "test-api-key");
            }
            ProviderConfig::Ollama(_) => panic!("expected openai provider config"),
        }
    }

    #[test]
    fn rejects_openai_provider_when_endpoint_is_missing() {
        let workflow = crate::parse_inline_workflow! {
            provider openai {
                driver: "openai"
                api_key: "test-api-key"
                models: ["model-a"]
            }
        };

        let provider_index_result = build_provider_index(&workflow);

        assert!(matches!(
            provider_index_result,
            Err(WorkflowRuntimeError::ProviderConfiguration { provider_name, message })
                if provider_name == "openai" && message.contains("missing `endpoint` property")
        ));
    }

    #[test]
    fn parses_ollama_provider_with_explicit_endpoint() {
        let workflow = crate::parse_inline_workflow! {
            provider ollama {
                driver: "ollama"
                endpoint: "http://127.0.0.1:11434"
                models: ["model-a"]
            }
        };

        let provider_index = build_provider_index(&workflow).expect("provider index should build");
        let provider = provider_index.get("ollama").expect("ollama provider should exist");

        match provider {
            ProviderConfig::OpenAI(_) => panic!("expected ollama provider config"),
            ProviderConfig::Ollama(ollama_provider_config) => {
                assert_eq!(ollama_provider_config.endpoint, "http://127.0.0.1:11434");
                assert_eq!(ollama_provider_config.host, "http://127.0.0.1");
                assert_eq!(ollama_provider_config.port, 11434);
            }
        }
    }

    #[test]
    fn rejects_ollama_provider_when_endpoint_port_is_missing() {
        let workflow = crate::parse_inline_workflow! {
            provider ollama {
                driver: "ollama"
                endpoint: "http://127.0.0.1"
                models: ["model-a"]
            }
        };

        let provider_index_result = build_provider_index(&workflow);

        assert!(matches!(
            provider_index_result,
            Err(WorkflowRuntimeError::ProviderConfiguration { provider_name, message })
                if provider_name == "ollama" && message.contains("explicit port")
        ));
    }
}
