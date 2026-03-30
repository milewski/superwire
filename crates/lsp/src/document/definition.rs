use engine_ai_core::dsl::{ReferenceKeyword, SourceSpan};

use crate::protocol::{Position, Range};

use super::position::source_span_to_range;
use super::reference::ReferenceCompletionPath;
use super::semantic_index::SemanticIndex;
use super::DocumentState;

impl DocumentState {
    #[must_use]
    pub fn definition_range(&self, position: Position) -> Option<Range> {
        let symbol_token = self.symbol_token_at(position)?;
        let definition_span = self
            .semantic_snapshot
            .semantic_index
            .definition_span_for_symbol(symbol_token.as_str())?;

        Some(source_span_to_range(&self.text, definition_span))
    }
}

impl SemanticIndex {
    fn definition_span_for_symbol(&self, symbol_token: &str) -> Option<SourceSpan> {
        if let Some(provider_span) = self.provider_span(symbol_token) {
            return Some(provider_span);
        }

        if let Some(schema_span) = self.schema_span(symbol_token) {
            return Some(schema_span);
        }

        if let Some(agent_span) = self.agent_span(symbol_token) {
            return Some(agent_span);
        }

        let reference_completion_path = ReferenceCompletionPath::from_token(symbol_token)?;

        if reference_completion_path.is_schema_root() {
            let schema_name = reference_completion_path.first_path_segment_or_pending()?;

            return self.schema_span(schema_name);
        }

        if let Some(reference_root_keyword) = reference_completion_path.root_keyword() {
            return match reference_root_keyword {
                ReferenceKeyword::Agent => {
                    let agent_name = reference_completion_path.first_path_segment_or_pending()?;

                    self.agent_span(agent_name)
                }
                ReferenceKeyword::Input | ReferenceKeyword::Secrets | ReferenceKeyword::Tool => None,
            };
        }

        self.provider_span(reference_completion_path.root_identifier())
    }

    fn provider_span(&self, provider_name: &str) -> Option<SourceSpan> {
        self.provider_locations
            .iter()
            .find(|provider_location| provider_location.name == provider_name)
            .map(|provider_location| provider_location.span)
    }

    fn schema_span(&self, schema_name: &str) -> Option<SourceSpan> {
        self.schema_locations
            .iter()
            .find(|schema_location| schema_location.name == schema_name)
            .map(|schema_location| schema_location.span)
    }

    fn agent_span(&self, agent_name: &str) -> Option<SourceSpan> {
        self.agent_locations
            .iter()
            .find(|agent_location| agent_location.name == agent_name)
            .map(|agent_location| agent_location.span)
    }
}

impl ReferenceCompletionPath {
    fn first_path_segment_or_pending(&self) -> Option<&str> {
        if let Some(first_access) = self.complete_accesses.first() {
            return Some(first_access);
        }

        if self.pending_prefix.is_empty() {
            return None;
        }

        Some(self.pending_prefix.as_str())
    }
}
