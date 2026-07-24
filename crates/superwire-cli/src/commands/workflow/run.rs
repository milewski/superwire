use std::fs;
use std::path::PathBuf;

use clap::Args;
use serde_json::Value;
use superwire_dsl::parse_workflow;
use superwire_executor::{AgentCacheDriver, AgentCacheOptions, AgentCacheSession, AgentCacheTimeToLive, ExecutorError, WorkflowExecutor};
use superwire_mcp::McpClientFactory;
use superwire_provider_cersei::{CerseiModelProvider, ProviderNetworkPolicy};
use superwire_semantic::WorkflowSemanticError;

use super::json::WorkflowPayloadSources;
use super::schema::CliRuntimeSchemaContext;
use crate::diagnostics::CommandError;

#[derive(Debug, Args)]
pub(super) struct RunWorkflowCommand {
    #[arg(value_name = "WORKFLOW_PATH")]
    workflow_path: PathBuf,

    #[arg(long, value_name = "JSON")]
    input_json: Option<String>,

    #[arg(long, value_name = "INPUT_JSON_FILE")]
    input_file: Option<PathBuf>,

    #[arg(long, value_name = "JSON")]
    secrets_json: Option<String>,

    #[arg(long, value_name = "SECRETS_JSON_FILE")]
    secrets_file: Option<PathBuf>,

    #[arg(long = "set", value_name = "KEY=VALUE", number_of_values = 1)]
    set: Option<Vec<String>>,

    #[arg(long, action = clap::ArgAction::SetTrue)]
    pretty: bool,

    #[arg(long, default_value_t = AgentCacheDriver::InMemory)]
    cache_driver: AgentCacheDriver,

    #[arg(long = "cache-ttl", default_value_t = AgentCacheTimeToLive::default())]
    cache_time_to_live: AgentCacheTimeToLive,

    #[arg(long = "no-cache", action = clap::ArgAction::SetFalse, default_value_t = true)]
    use_cache: bool,
}

impl RunWorkflowCommand {
    pub(super) fn execute_with_mcp_client_factory(self, mcp_client_factory: &dyn McpClientFactory) -> Result<(), CommandError> {
        self.payload_sources().validate()?;

        let input_value = Value::Object(self.payload_sources().input_value()?);
        let secrets_value = Value::Object(self.payload_sources().secrets_value()?);

        let async_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| CommandError::internal(format!("failed to build tokio runtime: {error}")))?;

        let workflow_source = fs::read_to_string(&self.workflow_path)
            .map_err(|error| CommandError::internal(format!("failed to read workflow file {}: {error}", self.workflow_path.display())))?;

        let parsed_workflow = parse_workflow(&workflow_source).map_err(|error| {
            let details = error.render_for_output_target(&workflow_source, &self.workflow_path.display().to_string());
            let semantic_error = WorkflowSemanticError::ParseFailed {
                source: Box::new(error),
                details,
            };

            Self::map_workflow_runtime_error(ExecutorError::Semantic(semantic_error))
        })?;

        let workflow_executor = if parsed_workflow
            .declarations()
            .iter()
            .any(|declaration| matches!(declaration, superwire_dsl::Declaration::McpServer(_)))
        {
            WorkflowExecutor::from_source_with_runtime_values_and_mcp_client_factory(
                &workflow_source,
                &input_value,
                &secrets_value,
                mcp_client_factory,
            )
        } else {
            WorkflowExecutor::from_source(&workflow_source)
        }
        .map_err(Self::map_workflow_runtime_error)?;
        let _runtime_schema_context = CliRuntimeSchemaContext::from_workflow(&parsed_workflow)?;
        let cache_options = self.cache_options()?;

        let model_provider = CerseiModelProvider::for_network_policy(ProviderNetworkPolicy::Trusted);

        let output_value = async_runtime
            .block_on(workflow_executor.execute_with_cache(input_value, secrets_value, &model_provider, None, 10, cache_options))
            .map_err(Self::map_workflow_runtime_error)?;

        if self.pretty {
            println!(
                "{}",
                serde_json::to_string_pretty(&output_value)
                    .map_err(|error| CommandError::internal(format!("failed to serialize pretty workflow output: {error}")))?
            );

            return Ok(());
        }

        println!(
            "{}",
            serde_json::to_string(&output_value)
                .map_err(|error| CommandError::internal(format!("failed to serialize workflow output: {error}")))?
        );

        Ok(())
    }

    fn payload_sources(&self) -> WorkflowPayloadSources<'_> {
        WorkflowPayloadSources::new(
            self.input_json.as_deref(),
            self.input_file.as_deref(),
            self.secrets_json.as_deref(),
            self.secrets_file.as_deref(),
            self.set.as_deref(),
        )
    }

    fn cache_options(&self) -> Result<AgentCacheOptions, CommandError> {
        if !self.use_cache {
            return Ok(AgentCacheOptions::disabled());
        }

        let cache_store = self
            .cache_driver
            .build_store()
            .map_err(|error| CommandError::internal(error.to_string()))?;

        Ok(AgentCacheOptions::enabled(
            AgentCacheSession::local(),
            cache_store,
            self.cache_time_to_live.0,
        ))
    }

    fn map_workflow_runtime_error(error: ExecutorError) -> CommandError {
        let message = error.to_string();
        let details = serde_json::to_value(error.diagnostic()).unwrap_or_else(|serialization_error| {
            serde_json::json!({
                "code": "internal_error",
                "message": format!("failed to serialize workflow diagnostic: {serialization_error}"),
            })
        });

        if error.is_client_error() {
            CommandError::invalid_input_with_details(message, details)
        } else {
            CommandError::internal_with_details(message, details)
        }
    }
}
