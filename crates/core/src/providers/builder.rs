use crate::ast::Provider as AstProvider;
use crate::providers::error::ProviderError;
use crate::providers::provider::ProviderRef;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Trait for building providers from AST configuration
pub trait ProviderBuilder: Send + Sync {
    /// Build a provider instance from AST configuration
    fn build(&self, provider: &AstProvider) -> Result<ProviderRef, ProviderError>;

    /// Get the driver name this builder handles
    fn driver_name(&self) -> &'static str;
}

/// Registry for provider builders following Open/Closed Principle
pub struct ProviderBuilderRegistry {
    builders: RwLock<HashMap<String, Arc<dyn ProviderBuilder>>>,
}

impl ProviderBuilderRegistry {
    /// Create a new empty registry
    #[must_use]
    pub fn new() -> Self {
        Self {
            builders: RwLock::new(HashMap::new()),
        }
    }

    /// Register a provider builder
    pub fn register(&self, builder: Arc<dyn ProviderBuilder>) {
        let driver_name = builder.driver_name().to_string();
        if let Ok(mut builders) = self.builders.write() {
            builders.insert(driver_name, builder);
        } else {
            log::error!("Failed to acquire write lock for provider builder registry");
        }
    }

    /// Build a provider from AST configuration
    pub fn build(&self, provider: &AstProvider) -> Result<ProviderRef, ProviderError> {
        let builders = self.builders.read().map_err(|_| ProviderError::ConnectionError {
            message: "Failed to acquire read lock for provider builder registry".to_string(),
            suggestion: None,
        })?;

        if let Some(builder) = builders.get(&provider.driver) {
            builder.build(provider)
        } else {
            let available: Vec<String> = builders.keys().cloned().collect();
            Err(ProviderError::ConnectionError {
                message: format!("Unknown provider driver: {}", provider.driver),
                suggestion: Some(format!("Supported drivers: {}", available.join(", "))),
            })
        }
    }

    /// Get list of registered driver names
    pub fn registered_drivers(&self) -> Vec<String> {
        if let Ok(builders) = self.builders.read() {
            builders.keys().cloned().collect()
        } else {
            log::error!("Failed to acquire read lock for provider builder registry");
            Vec::new()
        }
    }
}

impl Default for ProviderBuilderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global registry instance
static GLOBAL_REGISTRY: std::sync::OnceLock<ProviderBuilderRegistry> = std::sync::OnceLock::new();

/// Get the global provider builder registry
pub fn global_registry() -> &'static ProviderBuilderRegistry {
    GLOBAL_REGISTRY.get_or_init(|| {
        let registry = ProviderBuilderRegistry::new();

        // Register built-in providers
        registry.register(Arc::new(crate::providers::drivers::anthropic::AnthropicProviderBuilder));
        registry.register(Arc::new(crate::providers::drivers::ollama::OllamaProviderBuilder));
        registry.register(Arc::new(crate::providers::drivers::openai::OpenAiProviderBuilder));

        registry
    })
}
