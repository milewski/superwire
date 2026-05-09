use superwire_core::dsl::{
    AgentPropertyName, DeclarationKeyword, ForClauseKeyword, ImportKeyword, SingletonDeclarationKind, ToolPropertyName,
};
use superwire_core::semantic::InferenceSetting;

use super::text_utils::{is_identifier, trailing_identifier};
use super::{CompletionKind, CompletionSuggestion};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionScope {
    General,
    AgentProperties,
    ToolProperties,
    McpToolBatchImport,
    InferenceSettings,
    TypedDeclarations,
    DynamicValues,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopeBlock {
    Other,
    Agent,
    Tool,
    McpToolBatchImport,
    Inference,
    TypedDeclaration,
    Dynamic,
}

pub fn completion_scope_at_offset(source_text: &str, cursor_offset: usize) -> CompletionScope {
    let mut scope_blocks = Vec::<ScopeBlock>::new();
    let mut token_state = ScopeScannerTokenState::default();
    let mut string_state = ScopeScannerStringState::default();

    for character in source_text[..cursor_offset].chars() {
        if string_state.accept(character) {
            continue;
        }

        if character.is_ascii_alphanumeric() || character == '_' {
            token_state.current_identifier.push(character);
            continue;
        }

        token_state.flush_identifier();

        match character {
            ':' => {
                token_state.pending_property = token_state.recent_identifiers.last().cloned();
            }
            '{' => {
                let block_kind = token_state.block_for_open_brace(scope_blocks.last().copied());
                scope_blocks.push(block_kind);
                token_state.clear_after_brace();
            }
            '}' => {
                let _ = scope_blocks.pop();
                token_state.clear_after_brace();
            }
            '\n' | '\r' | ';' => {
                token_state.clear_after_statement();
            }
            ',' => {
                token_state.pending_property = None;
            }
            _ => {}
        }
    }

    match scope_blocks.last().copied() {
        Some(ScopeBlock::Inference) => CompletionScope::InferenceSettings,
        Some(ScopeBlock::Agent) => CompletionScope::AgentProperties,
        Some(ScopeBlock::Tool) => CompletionScope::ToolProperties,
        Some(ScopeBlock::McpToolBatchImport) => CompletionScope::McpToolBatchImport,
        Some(ScopeBlock::TypedDeclaration) => CompletionScope::TypedDeclarations,
        Some(ScopeBlock::Dynamic) => CompletionScope::DynamicValues,
        Some(ScopeBlock::Other) | None => CompletionScope::General,
    }
}

#[derive(Debug, Default)]
struct ScopeScannerTokenState {
    current_identifier: String,
    recent_identifiers: Vec<String>,
    pending_property: Option<String>,
}

impl ScopeScannerTokenState {
    fn flush_identifier(&mut self) {
        if self.current_identifier.is_empty() {
            return;
        }

        if !is_identifier(&self.current_identifier) {
            self.current_identifier.clear();

            return;
        }

        self.recent_identifiers.push(self.current_identifier.clone());
        self.current_identifier.clear();

        if self.recent_identifiers.len() > 6 {
            let _ = self.recent_identifiers.remove(0);
        }
    }

    fn block_for_open_brace(&self, parent_block: Option<ScopeBlock>) -> ScopeBlock {
        let Some(last_identifier) = self.recent_identifiers.last() else {
            return ScopeBlock::Other;
        };

        if parent_block == Some(ScopeBlock::TypedDeclaration) {
            return ScopeBlock::TypedDeclaration;
        }

        if parent_block == Some(ScopeBlock::Tool)
            && matches!(
                ToolPropertyName::from_identifier(last_identifier),
                Some(ToolPropertyName::Input | ToolPropertyName::Bindings | ToolPropertyName::Output)
            )
        {
            return ScopeBlock::TypedDeclaration;
        }

        if parent_block == Some(ScopeBlock::McpToolBatchImport) {
            match ToolPropertyName::from_identifier(last_identifier) {
                Some(ToolPropertyName::Input | ToolPropertyName::Output) => {
                    return ScopeBlock::TypedDeclaration;
                }
                Some(ToolPropertyName::Bindings) => {
                    return ScopeBlock::Other;
                }
                _ => {}
            }
        }

        if let Some(pending_property) = &self.pending_property {
            if parent_block == Some(ScopeBlock::Agent) {
                if let Some(agent_property_name) = AgentPropertyName::from_identifier(pending_property) {
                    if agent_property_name == AgentPropertyName::Inference {
                        return ScopeBlock::Inference;
                    }

                    if agent_property_name == AgentPropertyName::Output {
                        return ScopeBlock::TypedDeclaration;
                    }
                }
            }
        }

        if last_identifier == SingletonDeclarationKind::Input.as_str() || last_identifier == SingletonDeclarationKind::Secrets.as_str() {
            return ScopeBlock::TypedDeclaration;
        }

        if DeclarationKeyword::from_identifier(last_identifier) == Some(DeclarationKeyword::Dynamic) {
            return ScopeBlock::Dynamic;
        }

        if self.recent_identifiers.len() >= 4
            && self.recent_identifiers[self.recent_identifiers.len() - 4] == ImportKeyword::From.as_str()
            && self.recent_identifiers[self.recent_identifiers.len() - 3] == DeclarationKeyword::Mcp.as_str()
        {
            let import_kind = &self.recent_identifiers[self.recent_identifiers.len() - 1];

            if import_kind == DeclarationKeyword::Tool.as_str()
                || import_kind == DeclarationKeyword::Resource.as_str()
                || import_kind == DeclarationKeyword::Prompt.as_str()
            {
                return ScopeBlock::McpToolBatchImport;
            }
        }

        if let Some(agent_keyword_index) = self
            .recent_identifiers
            .iter()
            .position(|identifier| DeclarationKeyword::from_identifier(identifier) == Some(DeclarationKeyword::Agent))
        {
            if let Some(agent_name_identifier) = self.recent_identifiers.get(agent_keyword_index + 1) {
                if ForClauseKeyword::from_identifier(agent_name_identifier).is_none()
                    && DeclarationKeyword::from_identifier(agent_name_identifier).is_none()
                {
                    return ScopeBlock::Agent;
                }
            }
        }

        if let Some(schema_keyword_index) = self
            .recent_identifiers
            .iter()
            .position(|identifier| DeclarationKeyword::from_identifier(identifier) == Some(DeclarationKeyword::Schema))
        {
            if let Some(schema_name_identifier) = self.recent_identifiers.get(schema_keyword_index + 1) {
                if ForClauseKeyword::from_identifier(schema_name_identifier).is_none()
                    && DeclarationKeyword::from_identifier(schema_name_identifier).is_none()
                {
                    return ScopeBlock::TypedDeclaration;
                }
            }
        }

        if let Some(tool_keyword_index) = self
            .recent_identifiers
            .iter()
            .position(|identifier| DeclarationKeyword::from_identifier(identifier) == Some(DeclarationKeyword::Tool))
        {
            if let Some(tool_name_identifier) = self.recent_identifiers.get(tool_keyword_index + 1) {
                if ForClauseKeyword::from_identifier(tool_name_identifier).is_none()
                    && DeclarationKeyword::from_identifier(tool_name_identifier).is_none()
                {
                    return ScopeBlock::Tool;
                }
            }
        }

        ScopeBlock::Other
    }

    fn clear_after_brace(&mut self) {
        self.pending_property = None;
        self.recent_identifiers.clear();
        self.current_identifier.clear();
    }

    fn clear_after_statement(&mut self) {
        self.pending_property = None;
        self.recent_identifiers.clear();
        self.current_identifier.clear();
    }
}

#[derive(Debug, Default)]
struct ScopeScannerStringState {
    inside_string: bool,
    escaping: bool,
}

impl ScopeScannerStringState {
    fn accept(&mut self, character: char) -> bool {
        if self.inside_string {
            if self.escaping {
                self.escaping = false;
                return true;
            }

            if character == '\\' {
                self.escaping = true;
                return true;
            }

            if character == '"' {
                self.inside_string = false;
            }

            return true;
        }

        if character == '"' {
            self.inside_string = true;
            return true;
        }

        false
    }
}

trait AgentPropertyCompletionDoc {
    fn completion_detail(self) -> &'static str;

    fn completion_documentation(self) -> &'static str;
}

impl AgentPropertyCompletionDoc for AgentPropertyName {
    fn completion_detail(self) -> &'static str {
        match self {
            Self::Dynamic => "Dynamic block",
            Self::Model => "Model binding (required)",
            Self::Instruction => "Instruction expression (required)",
            Self::Output => "Output type",
            Self::Context => "Context expression",
            Self::Inference => "Inference settings object",
            Self::Uses => "Usable capabilities expression",
            Self::Unknown => "Unknown property",
        }
    }

    fn completion_documentation(self) -> &'static str {
        match self {
            Self::Dynamic => "Declares one or more dynamic values available as `dynamic.<field>`.",
            Self::Model => "Selects provider and model call used by this agent.",
            Self::Instruction => "Defines the instruction sent to the provider.",
            Self::Output => "Declares the expected structured output type.",
            Self::Context => "Prepends evaluated context to the rendered prompt.",
            Self::Inference => "Configures sampling and provider retry behavior.",
            Self::Uses => "Declares tool, MCP prompt, and MCP resource references available to this agent.",
            Self::Unknown => "Unsupported agent property.",
        }
    }
}

trait InferenceSettingCompletionDoc {
    fn completion_detail(self) -> &'static str;

    fn completion_documentation(self) -> &'static str;
}

impl InferenceSettingCompletionDoc for InferenceSetting {
    fn completion_detail(self) -> &'static str {
        match self {
            Self::MaxTokens => "Token budget (integer)",
            Self::Temperature => "Sampling temperature (number)",
            Self::TopP => "Nucleus sampling top_p (number)",
            Self::TopK => "Top-k sampling limit (integer)",
            Self::FrequencyPenalty => "Frequency penalty (number)",
            Self::PresencePenalty => "Presence penalty (number)",
            Self::RepeatPenalty => "Repeat penalty (number)",
            Self::Seed => "Random seed (integer)",
            Self::StuckThreshold => "Stuck retry threshold (integer)",
            Self::ProviderMaxRetries => "Provider max retries (integer)",
            Self::ProviderRetryBaseDelayMs => "Retry base delay ms (integer)",
        }
    }

    fn completion_documentation(self) -> &'static str {
        match self {
            Self::MaxTokens => "Maximum number of generated tokens.",
            Self::Temperature => "Controls randomness in token sampling.",
            Self::TopP => "Limits sampling to the smallest token set reaching cumulative probability `p`.",
            Self::TopK => "Limits sampling to the top `k` most likely tokens.",
            Self::FrequencyPenalty => "Penalizes tokens repeated frequently in generated output.",
            Self::PresencePenalty => "Penalizes tokens that already appeared in generated output.",
            Self::RepeatPenalty => "Applies multiplicative penalty to repeated tokens.",
            Self::Seed => "Sets deterministic random seed for repeatable generation.",
            Self::StuckThreshold => "Retry generation after this many stalled attempts.",
            Self::ProviderMaxRetries => "Maximum retries for provider-side failures.",
            Self::ProviderRetryBaseDelayMs => "Base backoff delay in milliseconds between provider retries.",
        }
    }
}

pub fn agent_property_scope_suggestions(line_prefix: &str) -> Vec<CompletionSuggestion> {
    let property_prefix = trailing_identifier(line_prefix).unwrap_or_default();

    AgentPropertyName::all()
        .into_iter()
        .filter(|agent_property_name| agent_property_name.as_str().starts_with(property_prefix))
        .map(|agent_property_name| CompletionSuggestion {
            label: agent_property_name.as_str().to_string(),
            kind: CompletionKind::Property,
            detail: agent_property_name.completion_detail().to_string(),
            documentation: agent_property_name.completion_documentation().to_string(),
            insert_text: agent_property_name.as_str().to_string(),
        })
        .collect()
}

pub fn inference_setting_scope_suggestions(line_prefix: &str) -> Vec<CompletionSuggestion> {
    let setting_prefix = trailing_identifier(line_prefix).unwrap_or_default();

    InferenceSetting::all()
        .into_iter()
        .filter(|inference_setting| inference_setting.key().starts_with(setting_prefix))
        .map(|inference_setting| CompletionSuggestion {
            label: inference_setting.key().to_string(),
            kind: CompletionKind::Property,
            detail: inference_setting.completion_detail().to_string(),
            documentation: inference_setting.completion_documentation().to_string(),
            insert_text: inference_setting.key().to_string(),
        })
        .collect()
}

pub fn tool_property_scope_suggestions(line_prefix: &str) -> Vec<CompletionSuggestion> {
    let property_prefix = trailing_identifier(line_prefix).unwrap_or_default();

    ToolPropertyName::all()
        .into_iter()
        .filter(|property_name| property_name.as_str().starts_with(property_prefix))
        .map(|property_name| CompletionSuggestion {
            label: property_name.as_str().to_string(),
            kind: CompletionKind::Property,
            detail: "Tool declaration property".to_string(),
            documentation: "Property available inside a `tool` declaration.".to_string(),
            insert_text: property_name.as_str().to_string(),
        })
        .collect()
}

pub fn mcp_tool_batch_import_scope_suggestions(line_prefix: &str) -> Vec<CompletionSuggestion> {
    let property_prefix = trailing_identifier(line_prefix).unwrap_or_default();

    [
        ToolPropertyName::Input.as_str(),
        ToolPropertyName::Bindings.as_str(),
        ToolPropertyName::MaxCalls.as_str(),
        ToolPropertyName::Output.as_str(),
        DeclarationKeyword::Tool.as_str(),
        DeclarationKeyword::Resource.as_str(),
        DeclarationKeyword::Prompt.as_str(),
    ]
    .into_iter()
    .filter(|property_name| property_name.starts_with(property_prefix))
    .map(|property_name| CompletionSuggestion {
        label: property_name.to_string(),
        kind: CompletionKind::Property,
        detail: "MCP tool batch import item".to_string(),
        documentation: "Shared binding, call limit, or tool item inside an MCP tool batch import.".to_string(),
        insert_text: property_name.to_string(),
    })
    .collect()
}
