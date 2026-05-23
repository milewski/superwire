use std::collections::HashMap;

use strsim::levenshtein;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyValueKind {
    Expression,
    ObjectBlock,
    TypedBlock,
    PlainString,
    UnsignedInteger,
    ModelUsage,
    ToolList,
    DynamicObject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PropertyDefinition {
    pub name: &'static str,
    pub value_kind: PropertyValueKind,
    pub required: bool,
    pub repeatable: bool,
    pub detail: &'static str,
    pub documentation: &'static str,
}

pub trait DslProperty {
    fn definition(&self) -> PropertyDefinition;
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Provider;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub id: ModelId,
    pub inference: Option<ModelInference>,
    pub assets: Option<ModelAssets>,
}

impl Model {
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: ModelId,
            inference: Some(ModelInference),
            assets: Some(ModelAssets),
        }
    }

    #[must_use]
    pub fn properties(&self) -> [PropertyDefinition; 3] {
        [
            self.id.definition(),
            self.inference
                .as_ref()
                .expect("model structure should include inference")
                .definition(),
            self.assets.as_ref().expect("model structure should include assets").definition(),
        ]
    }
}

impl Default for Model {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelUsage {
    pub inference: Option<ModelUsageInference>,
}

impl ModelUsage {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inference: Some(ModelUsageInference),
        }
    }

    #[must_use]
    pub fn properties(&self) -> [PropertyDefinition; 1] {
        [self
            .inference
            .as_ref()
            .expect("model usage structure should include inference")
            .definition()]
    }
}

impl Default for ModelUsage {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServer {
    pub endpoint: McpServerEndpoint,
    pub headers: Option<McpServerHeaders>,
}

impl McpServer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            endpoint: McpServerEndpoint,
            headers: Some(McpServerHeaders),
        }
    }

    #[must_use]
    pub fn properties(&self) -> [PropertyDefinition; 2] {
        [
            self.endpoint.definition(),
            self.headers
                .as_ref()
                .expect("mcp server structure should include headers")
                .definition(),
        ]
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tool {
    pub description: Option<ToolDescription>,
    pub max_calls: Option<ToolMaxCalls>,
    pub input: Option<ToolInput>,
    pub bindings: Option<ToolBindings>,
    pub output: Option<ToolOutput>,
}

impl Tool {
    #[must_use]
    pub fn new() -> Self {
        Self {
            description: Some(ToolDescription),
            max_calls: Some(ToolMaxCalls),
            input: Some(ToolInput),
            bindings: Some(ToolBindings),
            output: Some(ToolOutput),
        }
    }

    #[must_use]
    pub fn properties(&self) -> [PropertyDefinition; 5] {
        [
            self.description
                .as_ref()
                .expect("tool structure should include description")
                .definition(),
            self.max_calls
                .as_ref()
                .expect("tool structure should include max_calls")
                .definition(),
            self.input.as_ref().expect("tool structure should include input").definition(),
            self.bindings.as_ref().expect("tool structure should include bindings").definition(),
            self.output.as_ref().expect("tool structure should include output").definition(),
        ]
    }
}

impl Default for Tool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpImport {
    pub bindings: Option<McpImportBindings>,
}

impl McpImport {
    #[must_use]
    pub fn new() -> Self {
        Self {
            bindings: Some(McpImportBindings),
        }
    }
}

impl Default for McpImport {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub input: Option<ToolCallInput>,
    pub bindings: Option<ToolCallBindings>,
    pub max_calls: Option<ToolCallMaxCalls>,
}

impl ToolCall {
    #[must_use]
    pub fn new() -> Self {
        Self {
            input: Some(ToolCallInput),
            bindings: Some(ToolCallBindings),
            max_calls: Some(ToolCallMaxCalls),
        }
    }
}

impl Default for ToolCall {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolBatchImport {
    pub bindings: Option<ToolBindings>,
    pub max_calls: Option<ToolMaxCalls>,
}

impl McpToolBatchImport {
    #[must_use]
    pub fn new() -> Self {
        Self {
            bindings: Some(ToolBindings),
            max_calls: Some(ToolMaxCalls),
        }
    }
}

impl Default for McpToolBatchImport {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    pub model: AgentModel,
    pub instruction: AgentInstruction,
    pub context: Option<AgentContext>,
    pub uses: Vec<AgentUse>,
    pub output: Option<AgentOutput>,
    pub dynamic: Vec<AgentDynamic>,
}

impl Agent {
    #[must_use]
    pub fn new() -> Self {
        Self {
            model: AgentModel,
            instruction: AgentInstruction,
            context: Some(AgentContext),
            uses: vec![AgentUse],
            output: Some(AgentOutput),
            dynamic: vec![AgentDynamic::default()],
        }
    }

    #[must_use]
    pub fn properties(&self) -> [PropertyDefinition; 6] {
        [
            self.dynamic[0].definition(),
            self.model.definition(),
            self.instruction.definition(),
            self.output.as_ref().expect("agent structure should include output").definition(),
            self.context.as_ref().expect("agent structure should include context").definition(),
            self.uses[0].definition(),
        ]
    }

    #[must_use]
    pub fn property_definition(&self, property_name: &str) -> Option<PropertyDefinition> {
        self.properties()
            .into_iter()
            .find(|property_definition| property_definition.name == property_name)
    }

    #[must_use]
    pub fn suggested_property_definition(&self, property_name: &str) -> Option<PropertyDefinition> {
        if property_name.is_empty() {
            return None;
        }

        let mut closest_property_definition = None;
        let mut closest_distance = usize::MAX;

        for property_definition in self.properties() {
            let candidate_distance = levenshtein(property_name, property_definition.name);

            if candidate_distance < closest_distance {
                closest_property_definition = Some(property_definition);
                closest_distance = candidate_distance;
            }
        }

        if closest_distance > Self::max_typo_distance(property_name) {
            return None;
        }

        closest_property_definition
    }

    #[must_use]
    pub fn rendered_property_values(&self) -> String {
        let mut rendered_property_names = self
            .properties()
            .into_iter()
            .map(|property_definition| format!("`{}`", property_definition.name))
            .collect::<Vec<_>>();

        let last_property_name = rendered_property_names
            .pop()
            .expect("agent property names should include a last value");

        format!("{} or {last_property_name}", rendered_property_names.join(", "))
    }

    #[must_use]
    pub fn property_is_dynamic(&self, property_name: &str) -> bool {
        property_name == self.dynamic[0].definition().name
    }

    #[must_use]
    pub fn property_is_model(&self, property_name: &str) -> bool {
        property_name == self.model.definition().name
    }

    #[must_use]
    pub fn property_is_instruction(&self, property_name: &str) -> bool {
        property_name == self.instruction.definition().name
    }

    #[must_use]
    pub fn property_is_output(&self, property_name: &str) -> bool {
        property_name
            == self
                .output
                .as_ref()
                .expect("agent structure should include output")
                .definition()
                .name
    }

    #[must_use]
    pub fn property_is_context(&self, property_name: &str) -> bool {
        property_name
            == self
                .context
                .as_ref()
                .expect("agent structure should include context")
                .definition()
                .name
    }

    #[must_use]
    pub fn property_is_uses(&self, property_name: &str) -> bool {
        property_name == self.uses[0].definition().name
    }

    fn max_typo_distance(property_name: &str) -> usize {
        let property_name_length = property_name.chars().count();

        if property_name_length <= 4 {
            return 1;
        }

        if property_name_length <= 8 {
            return 2;
        }

        3
    }
}

impl Default for Agent {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModelId;

impl DslProperty for ModelId {
    fn definition(&self) -> PropertyDefinition {
        PropertyDefinition {
            name: "id",
            value_kind: PropertyValueKind::Expression,
            required: true,
            repeatable: false,
            detail: "Model id expression",
            documentation: "Defines the provider model identifier used for calls through this model profile.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModelInference;

impl DslProperty for ModelInference {
    fn definition(&self) -> PropertyDefinition {
        PropertyDefinition {
            name: "inference",
            value_kind: PropertyValueKind::ObjectBlock,
            required: false,
            repeatable: false,
            detail: "Default inference settings",
            documentation: "Defines default inference settings inherited by agents using this model profile.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModelAssets;

impl DslProperty for ModelAssets {
    fn definition(&self) -> PropertyDefinition {
        PropertyDefinition {
            name: "assets",
            value_kind: PropertyValueKind::Expression,
            required: false,
            repeatable: false,
            detail: "Supported asset kinds",
            documentation: "Declares which asset kinds this model profile can receive.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModelUsageInference;

impl DslProperty for ModelUsageInference {
    fn definition(&self) -> PropertyDefinition {
        PropertyDefinition {
            name: "inference",
            value_kind: PropertyValueKind::ObjectBlock,
            required: false,
            repeatable: false,
            detail: "Inference overrides",
            documentation: "Overrides model-profile inference settings for this agent only.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct McpServerEndpoint;

impl DslProperty for McpServerEndpoint {
    fn definition(&self) -> PropertyDefinition {
        PropertyDefinition {
            name: "endpoint",
            value_kind: PropertyValueKind::Expression,
            required: true,
            repeatable: false,
            detail: "MCP endpoint",
            documentation: "Defines the MCP server endpoint URL expression.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct McpServerHeaders;

impl DslProperty for McpServerHeaders {
    fn definition(&self) -> PropertyDefinition {
        PropertyDefinition {
            name: "headers",
            value_kind: PropertyValueKind::Expression,
            required: false,
            repeatable: false,
            detail: "MCP headers",
            documentation: "Defines optional MCP request headers as a block.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ToolDescription;

impl DslProperty for ToolDescription {
    fn definition(&self) -> PropertyDefinition {
        PropertyDefinition {
            name: "description",
            value_kind: PropertyValueKind::PlainString,
            required: false,
            repeatable: false,
            detail: "Tool description",
            documentation: "Provides the tool description exposed to agents and providers.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ToolMaxCalls;

impl DslProperty for ToolMaxCalls {
    fn definition(&self) -> PropertyDefinition {
        PropertyDefinition {
            name: "max_calls",
            value_kind: PropertyValueKind::UnsignedInteger,
            required: false,
            repeatable: false,
            detail: "Tool call limit",
            documentation: "Limits how many times the tool may be called in one agent execution.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ToolInput;

impl DslProperty for ToolInput {
    fn definition(&self) -> PropertyDefinition {
        PropertyDefinition {
            name: "input",
            value_kind: PropertyValueKind::TypedBlock,
            required: false,
            repeatable: false,
            detail: "Tool input schema",
            documentation: "Declares input fields accepted by the tool.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ToolBindings;

impl DslProperty for ToolBindings {
    fn definition(&self) -> PropertyDefinition {
        PropertyDefinition {
            name: "bindings",
            value_kind: PropertyValueKind::TypedBlock,
            required: false,
            repeatable: false,
            detail: "Tool bindings schema",
            documentation: "Declares fixed or caller-provided bindings required before the tool is invoked.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ToolOutput;

impl DslProperty for ToolOutput {
    fn definition(&self) -> PropertyDefinition {
        PropertyDefinition {
            name: "output",
            value_kind: PropertyValueKind::TypedBlock,
            required: false,
            repeatable: false,
            detail: "Tool output schema",
            documentation: "Declares fields returned by the tool.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct McpImportBindings;

impl DslProperty for McpImportBindings {
    fn definition(&self) -> PropertyDefinition {
        PropertyDefinition {
            name: "bindings",
            value_kind: PropertyValueKind::ObjectBlock,
            required: false,
            repeatable: false,
            detail: "MCP bindings",
            documentation: "Defines fixed MCP prompt, resource, or tool argument bindings.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ToolCallInput;

impl DslProperty for ToolCallInput {
    fn definition(&self) -> PropertyDefinition {
        PropertyDefinition {
            name: "input",
            value_kind: PropertyValueKind::ObjectBlock,
            required: false,
            repeatable: false,
            detail: "Tool call input",
            documentation: "Provides input values for this tool call.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ToolCallBindings;

impl DslProperty for ToolCallBindings {
    fn definition(&self) -> PropertyDefinition {
        PropertyDefinition {
            name: "bindings",
            value_kind: PropertyValueKind::ObjectBlock,
            required: false,
            repeatable: false,
            detail: "Tool call bindings",
            documentation: "Provides binding values for this tool call.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ToolCallMaxCalls;

impl DslProperty for ToolCallMaxCalls {
    fn definition(&self) -> PropertyDefinition {
        PropertyDefinition {
            name: "max_calls",
            value_kind: PropertyValueKind::UnsignedInteger,
            required: false,
            repeatable: false,
            detail: "Tool call limit",
            documentation: "Overrides how many times this call target may be invoked.",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentDynamicProperties {
    pub properties: HashMap<String, PropertyDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentDynamic {
    pub properties: AgentDynamicProperties,
}

impl DslProperty for AgentDynamic {
    fn definition(&self) -> PropertyDefinition {
        PropertyDefinition {
            name: "dynamic",
            value_kind: PropertyValueKind::DynamicObject,
            required: false,
            repeatable: true,
            detail: "Dynamic block",
            documentation: "Declares one or more dynamic values available as `dynamic.<field>`.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AgentModel;

impl DslProperty for AgentModel {
    fn definition(&self) -> PropertyDefinition {
        PropertyDefinition {
            name: "model",
            value_kind: PropertyValueKind::ModelUsage,
            required: true,
            repeatable: false,
            detail: "Model binding (required)",
            documentation: "Selects provider and model call used by this agent.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AgentInstruction;

impl DslProperty for AgentInstruction {
    fn definition(&self) -> PropertyDefinition {
        PropertyDefinition {
            name: "instruction",
            value_kind: PropertyValueKind::Expression,
            required: true,
            repeatable: false,
            detail: "Instruction expression (required)",
            documentation: "Defines the instruction sent to the provider.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AgentOutput;

impl DslProperty for AgentOutput {
    fn definition(&self) -> PropertyDefinition {
        PropertyDefinition {
            name: "output",
            value_kind: PropertyValueKind::TypedBlock,
            required: false,
            repeatable: false,
            detail: "Output type",
            documentation: "Declares the expected structured output type.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AgentContext;

impl DslProperty for AgentContext {
    fn definition(&self) -> PropertyDefinition {
        PropertyDefinition {
            name: "context",
            value_kind: PropertyValueKind::Expression,
            required: false,
            repeatable: false,
            detail: "Context expression",
            documentation: "Prepends evaluated context to the rendered prompt.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AgentUse;

impl DslProperty for AgentUse {
    fn definition(&self) -> PropertyDefinition {
        PropertyDefinition {
            name: "uses",
            value_kind: PropertyValueKind::ToolList,
            required: false,
            repeatable: false,
            detail: "Usable capabilities expression",
            documentation: "Declares tool, MCP prompt, and MCP resource references available to this agent.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Agent;

    #[test]
    fn suggests_closest_agent_property_name_for_typos() {
        let agent = Agent::new();

        assert_eq!(
            agent
                .suggested_property_definition("instrction")
                .map(|property_definition| property_definition.name),
            Some("instruction")
        );

        assert_eq!(
            agent
                .suggested_property_definition("modle")
                .map(|property_definition| property_definition.name),
            Some("model")
        );
    }

    #[test]
    fn does_not_suggest_agent_property_name_for_distant_identifier() {
        assert_eq!(Agent::new().suggested_property_definition("retries"), None);
    }
}
