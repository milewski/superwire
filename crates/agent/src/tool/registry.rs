use crate::tool::RuntimeTool;
use std::sync::Arc;

pub struct ToolRegistration {
    create_runtime_tool: fn() -> Arc<dyn RuntimeTool>,
}

impl ToolRegistration {
    pub const fn from_default<ToolType>() -> Self
    where
        ToolType: RuntimeTool + Default + 'static,
    {
        Self {
            create_runtime_tool: create_default_runtime_tool::<ToolType>,
        }
    }

    #[must_use]
    pub fn create_runtime_tool(&self) -> Arc<dyn RuntimeTool> {
        (self.create_runtime_tool)()
    }
}

fn create_default_runtime_tool<ToolType>() -> Arc<dyn RuntimeTool>
where
    ToolType: RuntimeTool + Default + 'static,
{
    Arc::new(ToolType::default())
}

inventory::collect!(ToolRegistration);

#[must_use]
pub fn registered_runtime_tools() -> Vec<Arc<dyn RuntimeTool>> {
    inventory::iter::<ToolRegistration>
        .into_iter()
        .map(ToolRegistration::create_runtime_tool)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::registered_runtime_tools;
    use crate::register_tool;
    use crate::tool::{Tool, ToolError};
    use async_trait::async_trait;
    use schemars::JsonSchema;
    use serde::Deserialize;
    use serde::Serialize;
    use serde_json::Value;

    #[derive(Debug, Clone, Default)]
    struct InventoryRegistryTestTool;

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    struct InventoryRegistryTestInput {
        value: String,
    }

    #[async_trait]
    impl Tool for InventoryRegistryTestTool {
        type Input = InventoryRegistryTestInput;

        fn name(&self) -> &str {
            "inventory_registry_test_tool"
        }

        fn description(&self) -> &str {
            "Tool used to validate inventory registration"
        }

        async fn execute(&self, input: Self::Input) -> Result<Value, ToolError> {
            Ok(serde_json::json!({ "value": input.value }))
        }
    }

    register_tool!(InventoryRegistryTestTool);

    #[test]
    fn discovers_tools_from_inventory() {
        let has_registered_tool = registered_runtime_tools().into_iter().any(|runtime_tool| {
            runtime_tool.definition().expect("tool definition should be available").name == "inventory_registry_test_tool"
        });

        assert!(has_registered_tool);
    }
}
