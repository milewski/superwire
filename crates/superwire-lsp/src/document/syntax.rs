use std::ops::Range;

use superwire_dsl::DeclarationKeyword;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexicalCompletionSite {
    Code,
    Comment,
    StringLiteral,
    MultilineStringLiteral,
    Interpolation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxPunctuation {
    OpenBrace,
    CloseBrace,
    OpenBracket,
    CloseBracket,
    OpenParenthesis,
    CloseParenthesis,
    Colon,
    Comma,
    Dot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxOperator {
    InterpolationOpen,
    InterpolationClose,
    OptionalAccess,
    Union,
    Assignment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxTokenKind {
    Identifier,
    Number,
    StringLiteral,
    MultilineStringLiteral,
    Comment,
    Punctuation(SyntaxPunctuation),
    Operator(SyntaxOperator),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxToken {
    pub kind: SyntaxTokenKind,
    pub byte_range: Range<usize>,
}

impl SyntaxToken {
    #[must_use]
    pub fn text<'source>(&self, source_text: &'source str) -> &'source str {
        source_text.get(self.byte_range.clone()).unwrap_or_default()
    }

    #[must_use]
    pub fn contains_or_touches(&self, byte_offset: usize) -> bool {
        self.byte_range.contains(&byte_offset) || self.byte_range.end == byte_offset
    }
}

#[derive(Debug, Clone)]
pub struct RecoveredDeclaration {
    pub keyword: DeclarationKeyword,
    pub name: Option<String>,
    pub byte_range: Range<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct SyntaxSnapshot {
    tokens: Vec<SyntaxToken>,
    recovered_declarations: Vec<RecoveredDeclaration>,
}

impl SyntaxSnapshot {
    #[must_use]
    pub fn from_source(source_text: &str) -> Self {
        let mut tokenizer = SyntaxTokenizer::new(source_text);
        tokenizer.scan_code(false);
        let tokens = tokenizer.tokens;
        let recovered_declarations = Self::recover_declarations(source_text, &tokens);

        Self {
            tokens,
            recovered_declarations,
        }
    }

    #[must_use]
    pub fn tokens(&self) -> &[SyntaxToken] {
        &self.tokens
    }

    #[must_use]
    pub fn recovered_declarations(&self) -> &[RecoveredDeclaration] {
        &self.recovered_declarations
    }

    #[must_use]
    pub fn token_at_offset(&self, byte_offset: usize) -> Option<&SyntaxToken> {
        self.tokens
            .iter()
            .find(|syntax_token| syntax_token.contains_or_touches(byte_offset))
    }

    #[must_use]
    pub fn completion_site_at_offset(&self, byte_offset: usize) -> LexicalCompletionSite {
        if let Some(syntax_token) = self.token_at_offset(byte_offset) {
            match syntax_token.kind {
                SyntaxTokenKind::Comment => return LexicalCompletionSite::Comment,
                SyntaxTokenKind::StringLiteral => return LexicalCompletionSite::StringLiteral,
                SyntaxTokenKind::MultilineStringLiteral => return LexicalCompletionSite::MultilineStringLiteral,
                SyntaxTokenKind::Identifier
                | SyntaxTokenKind::Number
                | SyntaxTokenKind::Punctuation(_)
                | SyntaxTokenKind::Operator(_)
                | SyntaxTokenKind::Unknown => {}
            }
        }

        let mut interpolation_depth = 0_usize;

        for syntax_token in self
            .tokens
            .iter()
            .take_while(|syntax_token| syntax_token.byte_range.start < byte_offset)
        {
            match syntax_token.kind {
                SyntaxTokenKind::Operator(SyntaxOperator::InterpolationOpen) => {
                    interpolation_depth = interpolation_depth.saturating_add(1);
                }
                SyntaxTokenKind::Operator(SyntaxOperator::InterpolationClose) => {
                    interpolation_depth = interpolation_depth.saturating_sub(1);
                }
                SyntaxTokenKind::Identifier
                | SyntaxTokenKind::Number
                | SyntaxTokenKind::StringLiteral
                | SyntaxTokenKind::MultilineStringLiteral
                | SyntaxTokenKind::Comment
                | SyntaxTokenKind::Punctuation(_)
                | SyntaxTokenKind::Operator(SyntaxOperator::OptionalAccess | SyntaxOperator::Union | SyntaxOperator::Assignment)
                | SyntaxTokenKind::Unknown => {}
            }
        }

        if interpolation_depth > 0 {
            LexicalCompletionSite::Interpolation
        } else {
            LexicalCompletionSite::Code
        }
    }

    fn recover_declarations(source_text: &str, tokens: &[SyntaxToken]) -> Vec<RecoveredDeclaration> {
        let code_tokens = tokens
            .iter()
            .filter(|syntax_token| {
                !matches!(
                    syntax_token.kind,
                    SyntaxTokenKind::Comment | SyntaxTokenKind::StringLiteral | SyntaxTokenKind::MultilineStringLiteral
                )
            })
            .collect::<Vec<_>>();
        let mut recovered_declarations = Vec::new();
        let mut token_index = 0_usize;

        while token_index < code_tokens.len() {
            let syntax_token = code_tokens[token_index];

            if syntax_token.kind != SyntaxTokenKind::Identifier {
                token_index = token_index.saturating_add(1);

                continue;
            }

            let Some(keyword) = DeclarationKeyword::from_identifier(syntax_token.text(source_text)) else {
                token_index = token_index.saturating_add(1);

                continue;
            };

            let name_token = code_tokens
                .get(token_index.saturating_add(1))
                .copied()
                .filter(|candidate_token| candidate_token.kind == SyntaxTokenKind::Identifier && keyword_requires_name(keyword));
            let byte_range_end = name_token.map_or(syntax_token.byte_range.end, |candidate_token| candidate_token.byte_range.end);

            recovered_declarations.push(RecoveredDeclaration {
                keyword,
                name: name_token.map(|candidate_token| candidate_token.text(source_text).to_string()),
                byte_range: syntax_token.byte_range.start..byte_range_end,
            });
            token_index = token_index.saturating_add(1);
        }

        recovered_declarations
    }
}

fn keyword_requires_name(keyword: DeclarationKeyword) -> bool {
    matches!(
        keyword,
        DeclarationKeyword::Provider
            | DeclarationKeyword::Model
            | DeclarationKeyword::Mcp
            | DeclarationKeyword::Schema
            | DeclarationKeyword::Tool
            | DeclarationKeyword::Resource
            | DeclarationKeyword::Prompt
            | DeclarationKeyword::Agent
    )
}

struct SyntaxTokenizer<'source> {
    source_text: &'source str,
    byte_offset: usize,
    tokens: Vec<SyntaxToken>,
}

impl<'source> SyntaxTokenizer<'source> {
    fn new(source_text: &'source str) -> Self {
        Self {
            source_text,
            byte_offset: 0,
            tokens: Vec::new(),
        }
    }

    fn scan_code(&mut self, stop_at_interpolation_close: bool) {
        while self.byte_offset < self.source_text.len() {
            if stop_at_interpolation_close && self.remaining_source().starts_with("}}") {
                self.push_fixed_token(2, SyntaxTokenKind::Operator(SyntaxOperator::InterpolationClose));

                return;
            }

            if self.remaining_source().starts_with("//") {
                self.scan_comment();

                continue;
            }

            if self.remaining_source().starts_with("\"\"\"") {
                self.scan_string(3, SyntaxTokenKind::MultilineStringLiteral);

                continue;
            }

            if self.remaining_source().starts_with('"') {
                self.scan_string(1, SyntaxTokenKind::StringLiteral);

                continue;
            }

            if self.remaining_source().starts_with("?.") {
                self.push_fixed_token(2, SyntaxTokenKind::Operator(SyntaxOperator::OptionalAccess));

                continue;
            }

            let Some(character) = self.current_character() else {
                break;
            };

            if character.is_whitespace() {
                self.advance_character();

                continue;
            }

            if is_identifier_start(character) {
                self.scan_identifier();

                continue;
            }

            if character.is_ascii_digit() {
                self.scan_number();

                continue;
            }

            let token_kind = match character {
                '{' => SyntaxTokenKind::Punctuation(SyntaxPunctuation::OpenBrace),
                '}' => SyntaxTokenKind::Punctuation(SyntaxPunctuation::CloseBrace),
                '[' => SyntaxTokenKind::Punctuation(SyntaxPunctuation::OpenBracket),
                ']' => SyntaxTokenKind::Punctuation(SyntaxPunctuation::CloseBracket),
                '(' => SyntaxTokenKind::Punctuation(SyntaxPunctuation::OpenParenthesis),
                ')' => SyntaxTokenKind::Punctuation(SyntaxPunctuation::CloseParenthesis),
                ':' => SyntaxTokenKind::Punctuation(SyntaxPunctuation::Colon),
                ',' => SyntaxTokenKind::Punctuation(SyntaxPunctuation::Comma),
                '.' => SyntaxTokenKind::Punctuation(SyntaxPunctuation::Dot),
                '|' => SyntaxTokenKind::Operator(SyntaxOperator::Union),
                '=' => SyntaxTokenKind::Operator(SyntaxOperator::Assignment),
                _ => SyntaxTokenKind::Unknown,
            };

            self.push_character_token(token_kind);
        }
    }

    fn scan_comment(&mut self) {
        let start_byte_offset = self.byte_offset;

        while let Some(character) = self.current_character() {
            if character == '\n' {
                break;
            }

            self.advance_character();
        }

        self.tokens.push(SyntaxToken {
            kind: SyntaxTokenKind::Comment,
            byte_range: start_byte_offset..self.byte_offset,
        });
    }

    fn scan_string(&mut self, delimiter_length: usize, token_kind: SyntaxTokenKind) {
        let delimiter = if delimiter_length == 3 { "\"\"\"" } else { "\"" };
        let mut segment_start_byte_offset = self.byte_offset;
        self.byte_offset = self.byte_offset.saturating_add(delimiter_length);

        while self.byte_offset < self.source_text.len() {
            if self.remaining_source().starts_with(delimiter) {
                self.byte_offset = self.byte_offset.saturating_add(delimiter_length);
                self.push_string_segment(segment_start_byte_offset, token_kind);

                return;
            }

            if self.remaining_source().starts_with("{{") {
                self.push_string_segment(segment_start_byte_offset, token_kind);
                self.push_fixed_token(2, SyntaxTokenKind::Operator(SyntaxOperator::InterpolationOpen));
                self.scan_code(true);
                segment_start_byte_offset = self.byte_offset;

                continue;
            }

            if self.remaining_source().starts_with('\\') {
                self.advance_character();

                if self.current_character().is_some() {
                    self.advance_character();
                }

                continue;
            }

            self.advance_character();
        }

        self.push_string_segment(segment_start_byte_offset, token_kind);
    }

    fn scan_identifier(&mut self) {
        let start_byte_offset = self.byte_offset;

        while self.current_character().is_some_and(is_identifier_continue) {
            self.advance_character();
        }

        self.tokens.push(SyntaxToken {
            kind: SyntaxTokenKind::Identifier,
            byte_range: start_byte_offset..self.byte_offset,
        });
    }

    fn scan_number(&mut self) {
        let start_byte_offset = self.byte_offset;
        let mut has_decimal_separator = false;

        while let Some(character) = self.current_character() {
            if character == '.' && !has_decimal_separator {
                has_decimal_separator = true;
                self.advance_character();

                continue;
            }

            if !character.is_ascii_digit() {
                break;
            }

            self.advance_character();
        }

        self.tokens.push(SyntaxToken {
            kind: SyntaxTokenKind::Number,
            byte_range: start_byte_offset..self.byte_offset,
        });
    }

    fn push_string_segment(&mut self, segment_start_byte_offset: usize, token_kind: SyntaxTokenKind) {
        if segment_start_byte_offset == self.byte_offset {
            return;
        }

        self.tokens.push(SyntaxToken {
            kind: token_kind,
            byte_range: segment_start_byte_offset..self.byte_offset,
        });
    }

    fn push_fixed_token(&mut self, token_length: usize, token_kind: SyntaxTokenKind) {
        let start_byte_offset = self.byte_offset;
        self.byte_offset = self.byte_offset.saturating_add(token_length).min(self.source_text.len());
        self.tokens.push(SyntaxToken {
            kind: token_kind,
            byte_range: start_byte_offset..self.byte_offset,
        });
    }

    fn push_character_token(&mut self, token_kind: SyntaxTokenKind) {
        let start_byte_offset = self.byte_offset;
        self.advance_character();
        self.tokens.push(SyntaxToken {
            kind: token_kind,
            byte_range: start_byte_offset..self.byte_offset,
        });
    }

    fn current_character(&self) -> Option<char> {
        self.remaining_source().chars().next()
    }

    fn remaining_source(&self) -> &str {
        self.source_text.get(self.byte_offset..).unwrap_or_default()
    }

    fn advance_character(&mut self) {
        let character_length = self.current_character().map_or(0, char::len_utf8);
        self.byte_offset = self.byte_offset.saturating_add(character_length);
    }
}

fn is_identifier_start(character: char) -> bool {
    character.is_ascii_alphabetic() || character == '_'
}

fn is_identifier_continue(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
}
