use crate::ast::*;

pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn execute(&self, args: serde_json::Value) -> anyhow::Result<serde_json::Value>;
}
