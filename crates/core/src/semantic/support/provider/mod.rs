use crate::dsl::{Declaration, Expression, ObjectField, ProviderDeclaration, Workflow};
use crate::semantic::support::expression::{evaluate_expression, EvaluationContext};
use crate::semantic::support::types::value_kind_name;
use crate::semantic::WorkflowSemanticError;
use std::collections::HashMap;

mod ollama;
mod openai;

pub use ollama::{OllamaProviderConfig, OllamaProviderConfigTemplate};
pub use openai::{OpenAIProviderConfig, OpenAIProviderConfigTemplate};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderDriver {
    OpenAI,
    Ollama,
}

impl ProviderDriver {
    #[must_use]
    pub fn all() -> [Self; 2] {
        [Self::OpenAI, Self::Ollama]
    }

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

    #[must_use]
    pub fn available_property_names(self) -> &'static [&'static str] {
        match self {
            Self::OpenAI => &OPENAI_PROVIDER_PROPERTIES,
            Self::Ollama => &OLLAMA_PROVIDER_PROPERTIES,
        }
    }

    #[must_use]
    pub fn supports_property(self, property_name: &str) -> bool {
        self.available_property_names().contains(&property_name)
    }
}

const OPENAI_PROVIDER_PROPERTIES: [&str; 4] = ["endpoint", "api_key", "organization", "project"];

const OLLAMA_PROVIDER_PROPERTIES: [&str; 1] = ["endpoint"];

pub trait ProviderConfigParser {
    fn parse(provider_declaration: &ProviderDeclaration) -> Result<ProviderConfigTemplate, WorkflowSemanticError>;

    fn required_property_expression(
        provider_declaration: &ProviderDeclaration,
        property_name: &str,
    ) -> Result<Expression, WorkflowSemanticError> {
        let Some(provider_property) = Self::provider_property_by_name(provider_declaration.properties.as_slice(), property_name) else {
            return Err(WorkflowSemanticError::ProviderConfiguration {
                provider_name: provider_declaration.name.clone(),
                message: format!("missing `{property_name}` property"),
            });
        };

        Ok(provider_property.value.clone())
    }

    fn required_string_literal_property(
        provider_declaration: &ProviderDeclaration,
        property_name: &str,
    ) -> Result<String, WorkflowSemanticError> {
        let property_expression = Self::required_property_expression(provider_declaration, property_name)?;
        let Expression::StringLiteral(string_literal) = property_expression else {
            return Err(WorkflowSemanticError::ProviderConfiguration {
                provider_name: provider_declaration.name.clone(),
                message: format!("`{property_name}` must be a string literal"),
            });
        };

        Ok(string_literal)
    }

    #[must_use]
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
pub enum ProviderConfigTemplate {
    OpenAI(OpenAIProviderConfigTemplate),
    Ollama(OllamaProviderConfigTemplate),
}

impl ProviderConfigTemplate {
    pub fn resolve(&self, provider_name: &str, evaluation_context: &EvaluationContext) -> Result<ProviderConfig, WorkflowSemanticError> {
        match self {
            Self::OpenAI(openai_provider_config_template) => {
                let openai_provider_config = openai_provider_config_template.resolve(provider_name, evaluation_context)?;

                Ok(ProviderConfig::OpenAI(openai_provider_config))
            }
            Self::Ollama(ollama_provider_config_template) => {
                let ollama_provider_config = ollama_provider_config_template.resolve(provider_name, evaluation_context)?;

                Ok(ProviderConfig::Ollama(ollama_provider_config))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderConfig {
    OpenAI(OpenAIProviderConfig),
    Ollama(OllamaProviderConfig),
}

impl Expression {
    fn evaluate_as_provider_string(
        &self,
        provider_name: &str,
        property_name: &str,
        evaluation_context: &EvaluationContext,
    ) -> Result<String, WorkflowSemanticError> {
        let evaluation_context_name = format!("provider `{provider_name}` property `{property_name}`");
        let evaluated_value = evaluate_expression(self, evaluation_context, &evaluation_context_name).map_err(|error| {
            WorkflowSemanticError::ProviderConfiguration {
                provider_name: provider_name.to_string(),
                message: format!("failed to evaluate `{property_name}`: {error}"),
            }
        })?;

        let Some(string_value) = evaluated_value.as_str() else {
            return Err(WorkflowSemanticError::ProviderConfiguration {
                provider_name: provider_name.to_string(),
                message: format!(
                    "`{property_name}` must resolve to a string, found {}",
                    value_kind_name(&evaluated_value)
                ),
            });
        };

        Ok(string_value.to_string())
    }
}

pub fn build_provider_index(workflow: &Workflow) -> Result<HashMap<String, ProviderConfigTemplate>, WorkflowSemanticError> {
    let mut provider_index = HashMap::new();

    for declaration in workflow.declarations() {
        let Declaration::Provider(provider_declaration) = declaration else {
            continue;
        };

        let provider_config_template = parse_provider_config(provider_declaration)?;
        provider_index.insert(provider_declaration.name.clone(), provider_config_template);
    }

    Ok(provider_index)
}

fn parse_provider_config(provider_declaration: &ProviderDeclaration) -> Result<ProviderConfigTemplate, WorkflowSemanticError> {
    let Some(provider_driver) = ProviderDriver::parse(&provider_declaration.driver_name) else {
        return Err(WorkflowSemanticError::ProviderConfiguration {
            provider_name: provider_declaration.name.clone(),
            message: format!("unsupported provider driver `{}`", provider_declaration.driver_name),
        });
    };

    match provider_driver {
        ProviderDriver::OpenAI => openai::OpenAIProviderConfigParser::parse(provider_declaration),
        ProviderDriver::Ollama => ollama::OllamaProviderConfigParser::parse(provider_declaration),
    }
}

#[cfg(test)]
mod tests {
    use super::{build_provider_index, ProviderConfig, ProviderConfigTemplate};
    use crate::semantic::support::expression::EvaluationContext;
    use crate::semantic::WorkflowSemanticError;
    use serde_json::{Map, Value};
    use std::collections::HashMap;

    fn empty_evaluation_context() -> EvaluationContext {
        EvaluationContext {
            input_values: Map::new(),
            secret_values: Map::new(),
            agent_outputs: HashMap::new(),
            agent_contexts: HashMap::new(),
            local_bindings: HashMap::new(),
        }
    }

    #[test]
    fn parses_openai_provider_with_required_endpoint_and_api_key() {
        let workflow = crate::parse_inline_workflow! {
            provider openai from openai {
endpoint: "https://api.openai.com/v1"
                api_key: "test-api-key"
}

model openai_model from openai {
    id: "model-a"
}
        };

        let provider_index = build_provider_index(&workflow).expect("provider index should build");
        let provider = provider_index.get("openai").expect("openai provider should exist");

        let resolved_provider_config = provider
            .resolve("openai", &empty_evaluation_context())
            .expect("provider config should resolve");

        match resolved_provider_config {
            ProviderConfig::OpenAI(openai_provider_config) => {
                assert_eq!(openai_provider_config.endpoint, "https://api.openai.com/v1");
                assert_eq!(openai_provider_config.api_key, "test-api-key");
            }
            ProviderConfig::Ollama(_) => panic!("expected openai provider config"),
        }
    }

    #[test]
    fn resolves_openai_provider_with_secret_references() {
        let workflow = crate::parse_inline_workflow! {
            secrets {
                endpoint: string
                api_key: string
            }

            provider openai from openai {
endpoint: secrets.endpoint
                api_key: secrets.api_key
}

model openai_model from openai {
    id: "model-a"
}
        };

        let provider_index = build_provider_index(&workflow).expect("provider index should build");
        let provider = provider_index.get("openai").expect("openai provider should exist");

        let mut evaluation_context = empty_evaluation_context();
        evaluation_context
            .secret_values
            .insert("endpoint".to_string(), Value::String("https://api.openai.com/v1".to_string()));
        evaluation_context
            .secret_values
            .insert("api_key".to_string(), Value::String("test-api-key".to_string()));

        let resolved_provider_config = provider
            .resolve("openai", &evaluation_context)
            .expect("provider config should resolve with secrets");

        match resolved_provider_config {
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
            provider openai from openai {
api_key: "test-api-key"
}

model openai_model from openai {
    id: "model-a"
}
        };

        let provider_index_result = build_provider_index(&workflow);

        assert!(matches!(
            provider_index_result,
            Err(WorkflowSemanticError::ProviderConfiguration { provider_name, message })
                if provider_name == "openai" && message.contains("missing `endpoint` property")
        ));
    }

    #[test]
    fn parses_ollama_provider_with_explicit_endpoint() {
        let workflow = crate::parse_inline_workflow! {
            provider ollama from ollama {
endpoint: "http://127.0.0.1:11434"
}

model ollama_model from ollama {
    id: "model-a"
}
        };

        let provider_index = build_provider_index(&workflow).expect("provider index should build");
        let provider = provider_index.get("ollama").expect("ollama provider should exist");

        let resolved_provider_config = provider
            .resolve("ollama", &empty_evaluation_context())
            .expect("provider config should resolve");

        match resolved_provider_config {
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
            provider ollama from ollama {
endpoint: "http://127.0.0.1"
}

model ollama_model from ollama {
    id: "model-a"
}
        };

        let provider_index = build_provider_index(&workflow).expect("provider index should build");
        let provider = provider_index.get("ollama").expect("ollama provider should exist");
        let provider_result = provider.resolve("ollama", &empty_evaluation_context());

        assert!(matches!(
            provider_result,
            Err(WorkflowSemanticError::ProviderConfiguration { provider_name, message })
                if provider_name == "ollama" && message.contains("explicit port")
        ));
    }

    #[test]
    fn builds_provider_templates() {
        let workflow = crate::parse_inline_workflow! {
            provider openai from openai {
endpoint: "https://api.openai.com/v1"
                api_key: "test-api-key"
}

model openai_model from openai {
    id: "model-a"
}

            provider ollama from ollama {
endpoint: "http://127.0.0.1:11434"
}

model ollama_model from ollama {
    id: "model-b"
}
        };

        let provider_index = build_provider_index(&workflow).expect("provider index should build");

        assert!(matches!(provider_index.get("openai"), Some(ProviderConfigTemplate::OpenAI(_))));
        assert!(matches!(provider_index.get("ollama"), Some(ProviderConfigTemplate::Ollama(_))));
    }
}
