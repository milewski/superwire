use crate::providers::error::ProviderError;
use crate::providers::provider::ProviderRef;
use std::collections::HashMap;

#[derive(Clone)]
pub struct ProviderRegistry {
    providers: HashMap<String, ProviderRef>,
}

impl ProviderRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    pub fn register(&mut self, name: String, provider: ProviderRef) {
        self.providers.insert(name, provider);
    }

    pub fn get(&self, name: &str) -> Result<ProviderRef, ProviderError> {
        self.providers
            .get(name)
            .cloned()
            .ok_or_else(|| ProviderError::ConnectionError {
                message: format!("Provider '{name}' not found"),
                suggestion: Some(format!(
                    "Available providers: {}",
                    self.providers.keys().cloned().collect::<Vec<_>>().join(", ")
                )),
            })
    }

    pub fn get_model_provider(&self, model_ref: &str) -> Result<(ProviderRef, String), ProviderError> {
        if let Some((provider_name, model_name)) = model_ref.split_once('/') {
            let provider = self.get(provider_name)?;

            if !provider.models().contains(&model_name.to_string()) {
                return Err(ProviderError::ModelNotFound {
                    model: model_name.to_string(),
                    available_models: provider.models().to_vec(),
                    suggestion: Some(format!(
                        "Available models for provider '{}': {}",
                        provider_name,
                        provider.models().join(", ")
                    )),
                });
            }

            Ok((provider, model_name.to_string()))
        } else {
            Err(ProviderError::ConnectionError {
                message: format!("Invalid model reference: '{model_ref}'. Expected format: 'provider/model'"),
                suggestion: Some("Use format 'provider_name/model_name'".to_string()),
            })
        }
    }

    #[must_use]
    pub fn list_providers(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
