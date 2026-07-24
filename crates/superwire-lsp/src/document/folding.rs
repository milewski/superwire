use super::syntax::{SyntaxPunctuation, SyntaxTokenKind};
use super::{DocumentState, FoldingRangeBlock};

impl DocumentState {
    #[must_use]
    pub fn folding_ranges(&self) -> Vec<FoldingRangeBlock> {
        let mut opening_brace_byte_offsets = Vec::new();
        let mut folding_ranges = Vec::new();

        for syntax_token in self.syntax_snapshot.tokens() {
            match syntax_token.kind {
                SyntaxTokenKind::Punctuation(SyntaxPunctuation::OpenBrace) => {
                    opening_brace_byte_offsets.push(syntax_token.byte_range.start);
                }
                SyntaxTokenKind::Punctuation(SyntaxPunctuation::CloseBrace) => {
                    let Some(opening_brace_byte_offset) = opening_brace_byte_offsets.pop() else {
                        continue;
                    };
                    let Some(start_position) = self.position_for_byte_offset(opening_brace_byte_offset) else {
                        continue;
                    };
                    let Some(end_position) = self.position_for_byte_offset(syntax_token.byte_range.start) else {
                        continue;
                    };

                    if start_position.line >= end_position.line {
                        continue;
                    }

                    folding_ranges.push(FoldingRangeBlock {
                        start_line: start_position.line,
                        start_character: start_position.character,
                        end_line: end_position.line,
                        end_character: end_position.character,
                    });
                }
                SyntaxTokenKind::Identifier
                | SyntaxTokenKind::Number
                | SyntaxTokenKind::StringLiteral
                | SyntaxTokenKind::MultilineStringLiteral
                | SyntaxTokenKind::Comment
                | SyntaxTokenKind::Punctuation(
                    SyntaxPunctuation::CloseBracket
                    | SyntaxPunctuation::OpenBracket
                    | SyntaxPunctuation::OpenParenthesis
                    | SyntaxPunctuation::CloseParenthesis
                    | SyntaxPunctuation::Colon
                    | SyntaxPunctuation::Comma
                    | SyntaxPunctuation::Dot,
                )
                | SyntaxTokenKind::Operator(_)
                | SyntaxTokenKind::Unknown => {}
            }
        }

        folding_ranges
    }
}
