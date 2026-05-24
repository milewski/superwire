use crate::dsl::{Declaration, Expression, ObjectField, ProviderDeclaration, Workflow};
use crate::semantic::support::expression::{evaluate_expression, EvaluationContext};
use crate::semantic::support::types::value_kind_name;
use crate::semantic::WorkflowSemanticError;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderDriver {
    Anthropic,
    OpenAi,
    Google,
    Mistral,
    Groq,
    DeepSeek,
    Xai,
    Together,
    Fireworks,
    Perplexity,
    Cerebras,
    Ollama,
    OpenRouter,
    Cohere,
    SambaNova,
    OpenAiCompatible,
    AnthropicCompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderApiFormat {
    Anthropic,
    Google,
    OpenAiCompatible,
}

impl ProviderDriver {
    #[must_use]
    pub fn all() -> [Self; 17] {
        [
            Self::Anthropic,
            Self::OpenAi,
            Self::Google,
            Self::Mistral,
            Self::Groq,
            Self::DeepSeek,
            Self::Xai,
            Self::Together,
            Self::Fireworks,
            Self::Perplexity,
            Self::Cerebras,
            Self::Ollama,
            Self::OpenRouter,
            Self::Cohere,
            Self::SambaNova,
            Self::OpenAiCompatible,
            Self::AnthropicCompatible,
        ]
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
            Self::Google => "google",
            Self::Mistral => "mistral",
            Self::Groq => "groq",
            Self::DeepSeek => "deepseek",
            Self::Xai => "xai",
            Self::Together => "together",
            Self::Fireworks => "fireworks",
            Self::Perplexity => "perplexity",
            Self::Cerebras => "cerebras",
            Self::Ollama => "ollama",
            Self::OpenRouter => "openrouter",
            Self::Cohere => "cohere",
            Self::SambaNova => "sambanova",
            Self::OpenAiCompatible => "openai_compatible",
            Self::AnthropicCompatible => "anthropic_compatible",
        }
    }

    #[must_use]
    pub fn parse(driver_name: &str) -> Option<Self> {
        Self::all()
            .into_iter()
            .find(|provider_driver| provider_driver.as_str() == driver_name)
    }

    #[must_use]
    pub fn available_property_names(self) -> &'static [&'static str] {
        &PROVIDER_PROPERTIES
    }

    #[must_use]
    pub fn supports_property(self, property_name: &str) -> bool {
        self.available_property_names().contains(&property_name)
    }

    #[must_use]
    pub fn api_format(self) -> ProviderApiFormat {
        match self {
            Self::Anthropic | Self::AnthropicCompatible => ProviderApiFormat::Anthropic,
            Self::Google => ProviderApiFormat::Google,
            Self::OpenAi
            | Self::Mistral
            | Self::Groq
            | Self::DeepSeek
            | Self::Xai
            | Self::Together
            | Self::Fireworks
            | Self::Perplexity
            | Self::Cerebras
            | Self::Ollama
            | Self::OpenRouter
            | Self::Cohere
            | Self::SambaNova
            | Self::OpenAiCompatible => ProviderApiFormat::OpenAiCompatible,
        }
    }

    #[must_use]
    pub fn default_endpoint(self) -> Option<&'static str> {
        match self {
            Self::Anthropic | Self::AnthropicCompatible => Some("https://api.anthropic.com"),
            Self::OpenAi | Self::OpenAiCompatible => Some("https://api.openai.com/v1"),
            Self::Google => Some("https://generativelanguage.googleapis.com/v1beta"),
            Self::Mistral => Some("https://api.mistral.ai/v1"),
            Self::Groq => Some("https://api.groq.com/openai/v1"),
            Self::DeepSeek => Some("https://api.deepseek.com/v1"),
            Self::Xai => Some("https://api.x.ai/v1"),
            Self::Together => Some("https://api.together.xyz/v1"),
            Self::Fireworks => Some("https://api.fireworks.ai/inference/v1"),
            Self::Perplexity => Some("https://api.perplexity.ai"),
            Self::Cerebras => Some("https://api.cerebras.ai/v1"),
            Self::Ollama => Some("http://localhost:11434/v1"),
            Self::OpenRouter => Some("https://openrouter.ai/api/v1"),
            Self::Cohere => Some("https://api.cohere.com/compatibility/v1"),
            Self::SambaNova => Some("https://api.sambanova.ai/v1"),
        }
    }

    #[must_use]
    pub fn api_key_environment_variables(self) -> &'static [&'static str] {
        match self {
            Self::Anthropic => &["ANTHROPIC_API_KEY", "ANTHROPIC_KEY"],
            Self::OpenAi => &["OPENAI_API_KEY"],
            Self::Google => &["GOOGLE_API_KEY", "GEMINI_API_KEY"],
            Self::Mistral => &["MISTRAL_API_KEY"],
            Self::Groq => &["GROQ_API_KEY"],
            Self::DeepSeek => &["DEEPSEEK_API_KEY"],
            Self::Xai => &["XAI_API_KEY"],
            Self::Together => &["TOGETHER_API_KEY"],
            Self::Fireworks => &["FIREWORKS_API_KEY"],
            Self::Perplexity => &["PERPLEXITY_API_KEY"],
            Self::Cerebras => &["CEREBRAS_API_KEY"],
            Self::Ollama | Self::OpenAiCompatible | Self::AnthropicCompatible => &[],
            Self::OpenRouter => &["OPENROUTER_API_KEY"],
            Self::Cohere => &["COHERE_API_KEY", "CO_API_KEY"],
            Self::SambaNova => &["SAMBANOVA_API_KEY"],
        }
    }

    #[must_use]
    pub fn requires_api_key(self) -> bool {
        self != Self::Ollama
    }
}

const PROVIDER_PROPERTIES: [&str; 2] = ["endpoint", "api_key"];

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
pub struct ProviderConfigTemplate {
    pub driver: ProviderDriver,
    pub endpoint_expression: Option<Expression>,
    pub api_key_expression: Option<Expression>,
}

impl ProviderConfigTemplate {
    pub fn resolve(&self, provider_name: &str, evaluation_context: &EvaluationContext) -> Result<ProviderConfig, WorkflowSemanticError> {
        let endpoint = self
            .endpoint_expression
            .as_ref()
            .map(|expression| expression.evaluate_as_provider_string(provider_name, "endpoint", evaluation_context))
            .transpose()?;
        let api_key = self
            .api_key_expression
            .as_ref()
            .map(|expression| expression.evaluate_as_provider_string(provider_name, "api_key", evaluation_context))
            .transpose()?;

        Ok(ProviderConfig {
            driver: self.driver,
            endpoint,
            api_key,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderConfig {
    pub driver: ProviderDriver,
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
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

    GenericProviderConfigParser::parse_with_driver(provider_declaration, provider_driver)
}

struct GenericProviderConfigParser;

impl GenericProviderConfigParser {
    fn parse_with_driver(
        provider_declaration: &ProviderDeclaration,
        driver: ProviderDriver,
    ) -> Result<ProviderConfigTemplate, WorkflowSemanticError> {
        Ok(ProviderConfigTemplate {
            driver,
            endpoint_expression: Self::optional_property_expression(provider_declaration, "endpoint"),
            api_key_expression: Self::optional_property_expression(provider_declaration, "api_key"),
        })
    }

    fn optional_property_expression(provider_declaration: &ProviderDeclaration, property_name: &str) -> Option<Expression> {
        provider_declaration
            .properties
            .iter()
            .find(|provider_property| provider_property.name == property_name)
            .map(|provider_property| provider_property.value.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::{build_provider_index, ProviderDriver};
    use crate::semantic::support::expression::EvaluationContext;
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
    fn parses_openai_provider_with_endpoint_and_api_key() {
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

        assert_eq!(resolved_provider_config.driver, ProviderDriver::OpenAi);
        assert_eq!(resolved_provider_config.endpoint.as_deref(), Some("https://api.openai.com/v1"));
        assert_eq!(resolved_provider_config.api_key.as_deref(), Some("test-api-key"));
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

        assert_eq!(resolved_provider_config.driver, ProviderDriver::OpenAi);
        assert_eq!(resolved_provider_config.endpoint.as_deref(), Some("https://api.openai.com/v1"));
        assert_eq!(resolved_provider_config.api_key.as_deref(), Some("test-api-key"));
    }

    #[test]
    fn parses_openai_provider_without_inline_credentials() {
        let workflow = crate::parse_inline_workflow! {
            provider openai from openai {
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

        assert_eq!(resolved_provider_config.driver, ProviderDriver::OpenAi);
        assert_eq!(resolved_provider_config.endpoint, None);
        assert_eq!(resolved_provider_config.api_key, None);
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

        assert_eq!(resolved_provider_config.driver, ProviderDriver::Ollama);
        assert_eq!(resolved_provider_config.endpoint.as_deref(), Some("http://127.0.0.1:11434"));
    }

    #[test]
    fn parses_custom_compatible_providers() {
        let workflow = crate::parse_inline_workflow! {
            provider local_openai from openai_compatible {
                endpoint: "http://127.0.0.1:8080/v1"
                api_key: "local-key"
            }

            model local_openai_model from local_openai {
                id: "model-a"
            }

            provider local_anthropic from anthropic_compatible {
                endpoint: "http://127.0.0.1:8081"
                api_key: "local-key"
            }

            model local_anthropic_model from local_anthropic {
                id: "model-b"
            }
        };

        let provider_index = build_provider_index(&workflow).expect("provider index should build");
        let openai_provider = provider_index
            .get("local_openai")
            .expect("openai-compatible provider should exist")
            .resolve("local_openai", &empty_evaluation_context())
            .expect("openai-compatible provider should resolve");
        let anthropic_provider = provider_index
            .get("local_anthropic")
            .expect("anthropic-compatible provider should exist")
            .resolve("local_anthropic", &empty_evaluation_context())
            .expect("anthropic-compatible provider should resolve");

        assert_eq!(openai_provider.driver, ProviderDriver::OpenAiCompatible);
        assert_eq!(anthropic_provider.driver, ProviderDriver::AnthropicCompatible);
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

        assert_eq!(
            provider_index.get("openai").map(|provider| provider.driver),
            Some(ProviderDriver::OpenAi)
        );
        assert_eq!(
            provider_index.get("ollama").map(|provider| provider.driver),
            Some(ProviderDriver::Ollama)
        );
    }
}
