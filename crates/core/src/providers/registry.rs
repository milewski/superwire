use std::collections::HashMap;
use std::sync::Arc;

use crate::ast::ProviderDefinition;
use crate::providers::error::ProviderError;
use crate::providers::provider::{Provider, ProviderModelConfig};

pub type DynProvider = Arc<dyn Provider>;

#[derive(Default)]
pub struct ProviderRegistry {
    providers: HashMap<String, DynProvider>,
}

impl ProviderRegistry {
    pub fn register(&mut self, driver: impl Into<String>, provider: DynProvider) {
        self.providers.insert(driver.into(), provider);
    }

    pub fn get(&self, driver: &str) -> Result<DynProvider, ProviderError> {
        self.providers
            .get(driver)
            .cloned()
            .ok_or_else(|| ProviderError::UnknownDriver {
                driver: driver.to_owned(),
            })
    }
}

pub fn resolve_model_config(
    provider: &ProviderDefinition,
    model_name: &str,
) -> ProviderModelConfig {
    ProviderModelConfig {
        provider_name: provider.name.clone(),
        model_name: model_name.to_owned(),
        api_endpoint: provider.api_endpoint.clone(),
    }
}
