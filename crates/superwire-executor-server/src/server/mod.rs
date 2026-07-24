mod error;
mod routes;
mod sse;

use superwire_mcp::McpNetworkPolicy;
use superwire_provider_cersei::ProviderNetworkPolicy;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExecutorServerConfig {
    mcp_network_policy: McpNetworkPolicy,
    provider_network_policy: ProviderNetworkPolicy,
}

impl ExecutorServerConfig {
    #[must_use]
    pub const fn new(mcp_network_policy: McpNetworkPolicy) -> Self {
        Self {
            mcp_network_policy,
            provider_network_policy: ProviderNetworkPolicy::BuiltInOnly,
        }
    }
    #[must_use]
    pub const fn with_provider_network_policy(mut self, provider_network_policy: ProviderNetworkPolicy) -> Self {
        self.provider_network_policy = provider_network_policy;
        self
    }

    #[must_use]
    pub const fn mcp_network_policy(self) -> McpNetworkPolicy {
        self.mcp_network_policy
    }
    #[must_use]
    pub const fn provider_network_policy(self) -> ProviderNetworkPolicy {
        self.provider_network_policy
    }
}

pub use routes::{
    executor_router, executor_router_with_service, executor_router_with_service_and_playground_dist, serve_executor,
    serve_executor_with_agent_cache, serve_executor_with_agent_cache_and_config, serve_executor_with_cache, serve_executor_with_config,
};
