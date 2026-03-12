use crate::ast::Provider as AstProvider;
use crate::providers::builder::global_registry;
use crate::providers::error::ProviderError;
use crate::providers::provider::ProviderRef;

pub struct ProviderFactory;

impl ProviderFactory {
    /// Create a provider using the global registry
    ///
    /// This method now delegates to the registration-based system,
    /// following the Open/Closed Principle. New providers can be added
    /// without modifying this code.
    pub fn create_provider(provider: &AstProvider) -> Result<ProviderRef, ProviderError> {
        global_registry().build(provider)
    }
}
