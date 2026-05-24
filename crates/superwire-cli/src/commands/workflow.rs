use clap::{Args, Subcommand};
use superwire_core::mcp::McpClientFactory;

use crate::diagnostics::CommandError;

mod check;
mod json;
mod lock;
mod paths;
mod prompt;
mod run;
mod schema;
mod vars;

use check::CheckWorkflowCommand;
use lock::LockWorkflowCommand;
use run::RunWorkflowCommand;
use vars::VarsWorkflowCommand;

#[derive(Debug, Args)]
pub struct WorkflowCommand {
    #[command(subcommand)]
    command: WorkflowSubcommand,
}

impl WorkflowCommand {
    pub fn execute_with_mcp_client_factory(self, mcp_client_factory: &dyn McpClientFactory) -> Result<(), CommandError> {
        match self.command {
            WorkflowSubcommand::Check(check_workflow_command) => check_workflow_command.execute(),
            WorkflowSubcommand::Run(run_workflow_command) => run_workflow_command.execute(),
            WorkflowSubcommand::Lock(lock_workflow_command) => lock_workflow_command.execute_with_mcp_client_factory(mcp_client_factory),
            WorkflowSubcommand::Vars(vars_workflow_command) => vars_workflow_command.execute(),
        }
    }
}

#[derive(Debug, Subcommand)]
enum WorkflowSubcommand {
    Check(CheckWorkflowCommand),
    Run(RunWorkflowCommand),
    #[command(
        after_help = "Examples:\n  superwire-cli workflow lock .\n  superwire-cli workflow lock workflows/*.wire --vars-file .wire.vars --output superwire.lock"
    )]
    Lock(LockWorkflowCommand),
    #[command(
        after_help = "Examples:\n  superwire-cli workflow vars .\n  superwire-cli workflow vars app/Services/Agent --output .wire.vars"
    )]
    Vars(VarsWorkflowCommand),
}
