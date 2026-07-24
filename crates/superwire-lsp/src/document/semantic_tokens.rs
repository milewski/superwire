use superwire_dsl::{
    AgentExpressionPropertyName, BuiltinFunctionName, DeclarationKeyword, ExpressionKeyword, ForClauseKeyword, ImportKeyword,
    ModelDeclarationPropertyName, ModelUsagePropertyName, ReferenceKeyword, ScalarTypeKeyword, ToolCallKeyword, ToolPropertyName,
};

use super::syntax::{SyntaxOperator, SyntaxPunctuation, SyntaxToken, SyntaxTokenKind};
use super::{DocumentState, SemanticHighlight, SemanticHighlightKind};

impl DocumentState {
    #[must_use]
    pub fn semantic_highlights(&self) -> Vec<SemanticHighlight> {
        let syntax_tokens = self.syntax_snapshot.tokens();
        let mut semantic_highlights = Vec::new();

        for (token_index, syntax_token) in syntax_tokens.iter().enumerate() {
            let previous_syntax_token = token_index
                .checked_sub(1)
                .and_then(|previous_index| syntax_tokens.get(previous_index));
            let next_syntax_token = syntax_tokens.get(token_index.saturating_add(1));
            let Some(highlight_kind) = self.highlight_kind_for_token(syntax_token, previous_syntax_token, next_syntax_token) else {
                continue;
            };

            semantic_highlights.extend(self.highlights_for_byte_range(syntax_token.byte_range.clone(), highlight_kind));
        }

        semantic_highlights
    }

    fn highlight_kind_for_token(
        &self,
        syntax_token: &SyntaxToken,
        previous_syntax_token: Option<&SyntaxToken>,
        next_syntax_token: Option<&SyntaxToken>,
    ) -> Option<SemanticHighlightKind> {
        match syntax_token.kind {
            SyntaxTokenKind::StringLiteral | SyntaxTokenKind::MultilineStringLiteral => Some(SemanticHighlightKind::String),
            SyntaxTokenKind::Number => Some(SemanticHighlightKind::Number),
            SyntaxTokenKind::Comment => Some(SemanticHighlightKind::Comment),
            SyntaxTokenKind::Operator(
                SyntaxOperator::InterpolationOpen
                | SyntaxOperator::InterpolationClose
                | SyntaxOperator::OptionalAccess
                | SyntaxOperator::Union
                | SyntaxOperator::Assignment,
            ) => Some(SemanticHighlightKind::Operator),
            SyntaxTokenKind::Identifier => {
                let identifier = syntax_token.text(&self.text);

                if let Some(declaration_keyword) = DeclarationKeyword::from_identifier(identifier) {
                    return Some(Self::declaration_keyword_highlight(declaration_keyword, previous_syntax_token));
                }

                if ImportKeyword::from_identifier(identifier).is_some()
                    || ForClauseKeyword::from_identifier(identifier).is_some()
                    || ExpressionKeyword::from_identifier(identifier).is_some()
                    || ToolCallKeyword::from_identifier(identifier).is_some()
                {
                    return Some(SemanticHighlightKind::Keyword);
                }

                if ScalarTypeKeyword::from_identifier(identifier).is_some() {
                    return Some(SemanticHighlightKind::Type);
                }

                if BuiltinFunctionName::from_identifier(identifier).is_some() {
                    return Some(SemanticHighlightKind::Function);
                }

                if Self::is_property_identifier(identifier, next_syntax_token) {
                    return Some(SemanticHighlightKind::Property);
                }

                if let Some(previous_identifier) = previous_syntax_token
                    .filter(|previous_token| previous_token.kind == SyntaxTokenKind::Identifier)
                    .map(|previous_token| previous_token.text(&self.text))
                    .and_then(DeclarationKeyword::from_identifier)
                {
                    return Some(Self::declaration_name_highlight(previous_identifier));
                }

                if ReferenceKeyword::from_identifier(identifier).is_some() {
                    return Some(SemanticHighlightKind::Namespace);
                }

                Some(SemanticHighlightKind::Variable)
            }
            SyntaxTokenKind::Punctuation(
                SyntaxPunctuation::OpenBrace
                | SyntaxPunctuation::CloseBrace
                | SyntaxPunctuation::OpenBracket
                | SyntaxPunctuation::CloseBracket
                | SyntaxPunctuation::OpenParenthesis
                | SyntaxPunctuation::CloseParenthesis
                | SyntaxPunctuation::Colon
                | SyntaxPunctuation::Comma
                | SyntaxPunctuation::Dot,
            )
            | SyntaxTokenKind::Unknown => None,
        }
    }

    fn declaration_keyword_highlight(
        declaration_keyword: DeclarationKeyword,
        previous_syntax_token: Option<&SyntaxToken>,
    ) -> SemanticHighlightKind {
        if previous_syntax_token.is_some_and(|previous_token| {
            matches!(
                previous_token.kind,
                SyntaxTokenKind::Punctuation(SyntaxPunctuation::Dot) | SyntaxTokenKind::Operator(SyntaxOperator::OptionalAccess)
            )
        }) {
            return SemanticHighlightKind::Property;
        }

        match declaration_keyword {
            DeclarationKeyword::Provider
            | DeclarationKeyword::Model
            | DeclarationKeyword::Mcp
            | DeclarationKeyword::Secrets
            | DeclarationKeyword::Input
            | DeclarationKeyword::Schema
            | DeclarationKeyword::Tool
            | DeclarationKeyword::Resource
            | DeclarationKeyword::Prompt
            | DeclarationKeyword::Dynamic
            | DeclarationKeyword::Agent
            | DeclarationKeyword::Output => SemanticHighlightKind::Keyword,
        }
    }

    fn declaration_name_highlight(declaration_keyword: DeclarationKeyword) -> SemanticHighlightKind {
        match declaration_keyword {
            DeclarationKeyword::Schema => SemanticHighlightKind::Type,
            DeclarationKeyword::Tool | DeclarationKeyword::Agent => SemanticHighlightKind::Function,
            DeclarationKeyword::Provider | DeclarationKeyword::Model | DeclarationKeyword::Mcp => SemanticHighlightKind::Class,
            DeclarationKeyword::Resource | DeclarationKeyword::Prompt => SemanticHighlightKind::Variable,
            DeclarationKeyword::Secrets | DeclarationKeyword::Input | DeclarationKeyword::Dynamic | DeclarationKeyword::Output => {
                SemanticHighlightKind::Namespace
            }
        }
    }

    fn is_property_identifier(identifier: &str, next_syntax_token: Option<&SyntaxToken>) -> bool {
        let known_property = AgentExpressionPropertyName::from_identifier(identifier).is_some()
            || ModelDeclarationPropertyName::from_identifier(identifier).is_some()
            || ModelUsagePropertyName::from_identifier(identifier).is_some()
            || ToolPropertyName::from_identifier(identifier).is_some();
        let followed_by_property_separator =
            next_syntax_token.is_some_and(|next_token| next_token.kind == SyntaxTokenKind::Punctuation(SyntaxPunctuation::Colon));

        known_property || followed_by_property_separator
    }

    fn highlights_for_byte_range(
        &self,
        byte_range: std::ops::Range<usize>,
        highlight_kind: SemanticHighlightKind,
    ) -> Vec<SemanticHighlight> {
        let mut semantic_highlights = Vec::new();
        let mut segment_start_byte_offset = byte_range.start;

        while segment_start_byte_offset < byte_range.end {
            let Some(segment_start_position) = self.position_for_byte_offset(segment_start_byte_offset) else {
                break;
            };
            let Some(line_byte_range) = self
                .line_index
                .line_content_byte_range(&self.text, segment_start_position.line as usize)
            else {
                break;
            };
            let segment_end_byte_offset = byte_range.end.min(line_byte_range.end);

            if segment_start_byte_offset < segment_end_byte_offset {
                if let Some(range) = self.range_for_byte_offsets(segment_start_byte_offset, segment_end_byte_offset) {
                    semantic_highlights.push(SemanticHighlight {
                        range,
                        kind: highlight_kind,
                    });
                }
            }

            if segment_end_byte_offset >= byte_range.end {
                break;
            }

            segment_start_byte_offset = self
                .line_index
                .next_line_start_byte_offset(segment_start_position.line as usize)
                .map_or(byte_range.end, |next_line_start| next_line_start.min(byte_range.end));
        }

        semantic_highlights
    }
}
