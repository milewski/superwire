use thiserror::Error;

use super::ast::{
    AgentDeclaration, AgentProperty, AgentPropertyName, CallArgument, Declaration, DeclarationKeyword, Expression, ForClauseKeyword,
    FunctionCall, ObjectField, Reference, StringTemplate, StringTemplatePart, TypeExpression, TypedField, Workflow,
};
use super::parse_workflow;
use super::parser::DslParseError;

#[derive(Debug, Error)]
pub enum DslFormatError {
    #[error("failed to parse DSL while formatting: {0}")]
    Parse(#[from] DslParseError),

    #[error("formatting rejects line comments (`//`) at line {line}, column {column}")]
    LineCommentNotAllowed { line: usize, column: usize },
}

pub fn format_workflow_source(source: &str) -> Result<String, DslFormatError> {
    source.ensure_no_line_comments()?;

    let workflow = parse_workflow(source)?;
    let mut formatter = DslFormatter::new();
    formatter.push_workflow(&workflow);

    if !formatter.output.ends_with('\n') {
        formatter.push_newline();
    }

    Ok(formatter.finish())
}

trait CommentPolicy {
    fn ensure_no_line_comments(&self) -> Result<(), DslFormatError>;
}

impl CommentPolicy for str {
    fn ensure_no_line_comments(&self) -> Result<(), DslFormatError> {
        let mut scanner = CommentScanner::new(self);
        scanner.ensure_no_line_comments()
    }
}

struct CommentScanner<'source> {
    source: &'source str,
    line: usize,
    column: usize,
    scanner_state: ScannerState,
}

impl<'source> CommentScanner<'source> {
    fn new(source: &'source str) -> Self {
        Self {
            source,
            line: 1,
            column: 1,
            scanner_state: ScannerState::Normal,
        }
    }

    fn ensure_no_line_comments(&mut self) -> Result<(), DslFormatError> {
        let source_bytes = self.source.as_bytes();
        let mut byte_index = 0;

        while byte_index < source_bytes.len() {
            if self.scanner_state == ScannerState::Normal && self.starts_with_at(byte_index, b"//") {
                return Err(DslFormatError::LineCommentNotAllowed {
                    line: self.line,
                    column: self.column,
                });
            }

            if self.scanner_state == ScannerState::Normal && self.starts_with_at(byte_index, b"\"\"\"") {
                self.scanner_state = ScannerState::MultilineString;
                self.advance_bytes(3, byte_index);
                byte_index += 3;
                continue;
            }

            if self.scanner_state == ScannerState::MultilineString && self.starts_with_at(byte_index, b"\"\"\"") {
                self.scanner_state = ScannerState::Normal;
                self.advance_bytes(3, byte_index);
                byte_index += 3;
                continue;
            }

            let remaining_source = &self.source[byte_index..];
            let character = remaining_source
                .chars()
                .next()
                .expect("remaining source should include a character");

            if self.scanner_state == ScannerState::Normal && character == '"' {
                self.scanner_state = ScannerState::QuotedString;
            } else if self.scanner_state == ScannerState::QuotedString && character == '"' {
                self.scanner_state = ScannerState::Normal;
            } else if self.scanner_state == ScannerState::QuotedString && character == '\\' {
                self.advance_character(character);
                byte_index += character.len_utf8();

                if byte_index < source_bytes.len() {
                    let escaped_character = self.source[byte_index..].chars().next().expect("escaped character should exist");

                    self.advance_character(escaped_character);
                    byte_index += escaped_character.len_utf8();
                }

                continue;
            }

            self.advance_character(character);
            byte_index += character.len_utf8();
        }

        Ok(())
    }

    fn starts_with_at(&self, byte_index: usize, pattern: &[u8]) -> bool {
        self.source
            .as_bytes()
            .get(byte_index..byte_index + pattern.len())
            .is_some_and(|window| window == pattern)
    }

    fn advance_bytes(&mut self, count: usize, byte_index: usize) {
        for character in self.source[byte_index..byte_index + count].chars() {
            self.advance_character(character);
        }
    }

    fn advance_character(&mut self, character: char) {
        if character == '\n' {
            self.line += 1;
            self.column = 1;
            return;
        }

        self.column += 1;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScannerState {
    Normal,
    QuotedString,
    MultilineString,
}

struct DslFormatter {
    output: String,
    indentation_depth: usize,
}

impl DslFormatter {
    fn new() -> Self {
        Self {
            output: String::new(),
            indentation_depth: 0,
        }
    }

    fn finish(self) -> String {
        self.output
    }

    fn push_workflow(&mut self, workflow: &Workflow) {
        let mut declaration_iterator = workflow.declarations.iter().peekable();

        while let Some(declaration) = declaration_iterator.next() {
            declaration.push_to_formatter(self);

            if declaration_iterator.peek().is_some() {
                self.push_newline();
            }
        }
    }

    fn push_declaration_block_start(&mut self, header: &str) {
        self.push_line(&format!("{header} {{"));
        self.indentation_depth += 1;
    }

    fn push_declaration_block_end(&mut self) {
        self.indentation_depth -= 1;
        self.push_line("}");
    }

    fn push_indent(&mut self) {
        for _ in 0..self.indentation_depth {
            self.output.push_str("    ");
        }
    }

    fn push_line(&mut self, line: &str) {
        self.push_indent();
        self.output.push_str(line);
        self.push_newline();
    }

    fn push_newline(&mut self) {
        self.output.push('\n');
    }

    fn push_expression_inline_string(&self, expression: &Expression) -> String {
        let mut inline_formatter = DslFormatter::new();
        expression.push_to_formatter(&mut inline_formatter, ExpressionFormat::Inline);
        inline_formatter.finish()
    }
}

impl Declaration {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        match self {
            Self::Provider(provider_declaration) => {
                formatter.push_declaration_block_start(&format!("{} {}", DeclarationKeyword::Provider.as_str(), provider_declaration.name));

                for object_field in &provider_declaration.properties {
                    object_field.push_to_formatter(formatter);
                }

                formatter.push_declaration_block_end();
            }
            Self::Secrets(secrets_declaration) => {
                formatter.push_declaration_block_start(DeclarationKeyword::Secrets.as_str());

                for typed_field in &secrets_declaration.fields {
                    typed_field.push_to_formatter(formatter);
                }

                formatter.push_declaration_block_end();
            }
            Self::Input(input_declaration) => {
                formatter.push_declaration_block_start(DeclarationKeyword::Input.as_str());

                for typed_field in &input_declaration.fields {
                    typed_field.push_to_formatter(formatter);
                }

                formatter.push_declaration_block_end();
            }
            Self::Schema(schema_declaration) => {
                formatter.push_declaration_block_start(&format!("{} {}", DeclarationKeyword::Schema.as_str(), schema_declaration.name));

                for typed_field in &schema_declaration.fields {
                    typed_field.push_to_formatter(formatter);
                }

                formatter.push_declaration_block_end();
            }
            Self::Agent(agent_declaration) => {
                agent_declaration.push_to_formatter(formatter);
            }
            Self::Output(output_declaration) => {
                formatter.push_declaration_block_start(DeclarationKeyword::Output.as_str());

                for object_field in &output_declaration.fields {
                    object_field.push_to_formatter(formatter);
                }

                formatter.push_declaration_block_end();
            }
        }
    }
}

impl AgentDeclaration {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        let mut header = format!("{} {}", DeclarationKeyword::Agent.as_str(), self.name);

        if let Some(for_loop) = &self.for_loop {
            header.push(' ');
            header.push_str(ForClauseKeyword::For.as_str());
            header.push(' ');
            header.push_str(&for_loop.iterator_name);
            header.push(' ');
            header.push_str(ForClauseKeyword::In.as_str());
            header.push(' ');
            header.push_str(&formatter.push_expression_inline_string(&for_loop.iterable));
        }

        formatter.push_declaration_block_start(&header);

        for agent_property in &self.properties {
            agent_property.push_to_formatter(formatter);
        }

        formatter.push_declaration_block_end();
    }
}

impl AgentProperty {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        match self {
            Self::Model(expression) => formatter.push_agent_property_expression(AgentPropertyName::Model.as_str(), expression),
            Self::Prompt(expression) => formatter.push_agent_property_expression(AgentPropertyName::Prompt.as_str(), expression),
            Self::Output(type_expression) => formatter.push_agent_property_type(AgentPropertyName::Output.as_str(), type_expression),
            Self::Context(expression) => formatter.push_agent_property_expression(AgentPropertyName::Context.as_str(), expression),
            Self::Inference(expression) => formatter.push_agent_property_expression(AgentPropertyName::Inference.as_str(), expression),
            Self::Tools(expression) => formatter.push_agent_property_expression(AgentPropertyName::Tools.as_str(), expression),
            Self::Custom { name, value } => formatter.push_agent_property_expression(name, value),
        }
    }
}

impl DslFormatter {
    fn push_agent_property_expression(&mut self, property_name: &str, expression: &Expression) {
        self.push_indent();
        self.output.push_str(property_name);
        self.output.push_str(": ");
        expression.push_to_formatter(self, ExpressionFormat::Canonical);
        self.push_newline();
    }

    fn push_agent_property_type(&mut self, property_name: &str, type_expression: &TypeExpression) {
        self.push_indent();
        self.output.push_str(property_name);
        self.output.push_str(": ");
        type_expression.push_to_formatter(self);
        self.push_newline();
    }
}

impl TypedField {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        formatter.push_indent();
        formatter.output.push_str(&self.name);
        formatter.output.push_str(": ");
        self.field_type.push_to_formatter(formatter);

        if let Some(description) = &self.description {
            formatter.output.push(' ');
            formatter.output.push_str(&render_string_literal(description));
        }

        formatter.push_newline();
    }
}

impl TypeExpression {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        match self {
            Self::String => formatter.output.push_str("string"),
            Self::Number => formatter.output.push_str("number"),
            Self::Float => formatter.output.push_str("float"),
            Self::Boolean => formatter.output.push_str("boolean"),
            Self::Null => formatter.output.push_str("null"),
            Self::SchemaReference(schema_name) => {
                formatter.output.push_str("schema.");
                formatter.output.push_str(schema_name);
            }
            Self::StringEnum(enum_value) => formatter.output.push_str(&render_string_literal(enum_value)),
            Self::Array { item_type, fixed_length } => {
                formatter.output.push('[');
                item_type.push_to_formatter(formatter);

                if let Some(array_length) = fixed_length {
                    formatter.output.push_str("; ");
                    formatter.output.push_str(&array_length.to_string());
                }

                formatter.output.push(']');
            }
            Self::Tuple(tuple_items) => {
                formatter.output.push('(');

                let mut tuple_iterator = tuple_items.iter().peekable();
                while let Some(tuple_item) = tuple_iterator.next() {
                    tuple_item.push_to_formatter(formatter);

                    if tuple_iterator.peek().is_some() {
                        formatter.output.push_str(", ");
                    }
                }

                formatter.output.push(')');
            }
            Self::Object(object_fields) => {
                formatter.output.push('{');
                formatter.push_newline();
                formatter.indentation_depth += 1;

                for typed_field in object_fields {
                    typed_field.push_to_formatter(formatter);
                }

                formatter.indentation_depth -= 1;
                formatter.push_indent();
                formatter.output.push('}');
            }
            Self::Union(union_members) => {
                let mut union_member_iterator = union_members.iter().peekable();

                while let Some(union_member) = union_member_iterator.next() {
                    union_member.push_to_formatter(formatter);

                    if union_member_iterator.peek().is_some() {
                        formatter.output.push_str(" | ");
                    }
                }
            }
        }
    }
}

impl ObjectField {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        formatter.push_indent();
        formatter.output.push_str(&self.name);
        formatter.output.push_str(": ");
        self.value.push_to_formatter(formatter, ExpressionFormat::Canonical);
        formatter.push_newline();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExpressionFormat {
    Canonical,
    Inline,
}

impl Expression {
    fn push_to_formatter(&self, formatter: &mut DslFormatter, expression_format: ExpressionFormat) {
        match self {
            Self::StringLiteral(string_value) => formatter.output.push_str(&render_string_literal(string_value)),
            Self::StringTemplate(string_template) => string_template.push_to_formatter(formatter),
            Self::NumberLiteral(number_literal) => formatter.output.push_str(number_literal),
            Self::BooleanLiteral(boolean_value) => {
                if *boolean_value {
                    formatter.output.push_str("true");
                } else {
                    formatter.output.push_str("false");
                }
            }
            Self::NullLiteral => formatter.output.push_str("null"),
            Self::Reference(reference) => reference.push_to_formatter(formatter),
            Self::FunctionCall(function_call) => function_call.push_to_formatter(formatter),
            Self::ArrayLiteral(array_items) => {
                if expression_format == ExpressionFormat::Inline {
                    formatter.output.push('[');

                    let mut item_iterator = array_items.iter().peekable();
                    while let Some(array_item) = item_iterator.next() {
                        array_item.push_to_formatter(formatter, ExpressionFormat::Inline);

                        if item_iterator.peek().is_some() {
                            formatter.output.push_str(", ");
                        }
                    }

                    formatter.output.push(']');
                    return;
                }

                if array_items.is_empty() {
                    formatter.output.push_str("[]");
                    return;
                }

                formatter.output.push('[');
                formatter.push_newline();
                formatter.indentation_depth += 1;

                for array_item in array_items {
                    formatter.push_indent();
                    array_item.push_to_formatter(formatter, ExpressionFormat::Canonical);
                    formatter.output.push(',');
                    formatter.push_newline();
                }

                formatter.indentation_depth -= 1;
                formatter.push_indent();
                formatter.output.push(']');
            }
            Self::ObjectLiteral(object_fields) => {
                if expression_format == ExpressionFormat::Inline {
                    formatter.output.push('{');

                    if !object_fields.is_empty() {
                        formatter.output.push(' ');
                    }

                    let mut field_iterator = object_fields.iter().peekable();
                    while let Some(object_field) = field_iterator.next() {
                        formatter.output.push_str(&object_field.name);
                        formatter.output.push_str(": ");
                        object_field.value.push_to_formatter(formatter, ExpressionFormat::Inline);

                        if field_iterator.peek().is_some() {
                            formatter.output.push(' ');
                        }
                    }

                    if !object_fields.is_empty() {
                        formatter.output.push(' ');
                    }

                    formatter.output.push('}');
                    return;
                }

                if object_fields.is_empty() {
                    formatter.output.push_str("{}");
                    return;
                }

                formatter.output.push('{');
                formatter.push_newline();
                formatter.indentation_depth += 1;

                for object_field in object_fields {
                    object_field.push_to_formatter(formatter);
                }

                formatter.indentation_depth -= 1;
                formatter.push_indent();
                formatter.output.push('}');
            }
        }
    }

    fn is_inline_friendly(&self) -> bool {
        match self {
            Self::ArrayLiteral(_) | Self::ObjectLiteral(_) => false,
            Self::StringLiteral(_)
            | Self::StringTemplate(_)
            | Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::Reference(_)
            | Self::FunctionCall(_) => true,
        }
    }
}

impl StringTemplate {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        let is_multiline = self
            .parts
            .iter()
            .any(|template_part| matches!(template_part, StringTemplatePart::Text(text) if text.contains('\n')));

        if is_multiline {
            formatter.output.push_str("\"\"\"");

            for template_part in &self.parts {
                match template_part {
                    StringTemplatePart::Text(text) => formatter.output.push_str(&escape_multiline_string_text(text)),
                    StringTemplatePart::Interpolation(expression) => {
                        formatter.output.push_str("{{ ");
                        expression.push_to_formatter(formatter, ExpressionFormat::Inline);
                        formatter.output.push_str(" }}");
                    }
                }
            }

            formatter.output.push_str("\"\"\"");
            return;
        }

        formatter.output.push('"');

        for template_part in &self.parts {
            match template_part {
                StringTemplatePart::Text(text) => formatter.output.push_str(&escape_quoted_string_text(text)),
                StringTemplatePart::Interpolation(expression) => {
                    formatter.output.push_str("{{ ");
                    expression.push_to_formatter(formatter, ExpressionFormat::Inline);
                    formatter.output.push_str(" }}");
                }
            }
        }

        formatter.output.push('"');
    }
}

impl Reference {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        formatter.output.push_str(&self.render_path());
    }
}

impl FunctionCall {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        self.callee.push_to_formatter(formatter);

        if self.arguments.is_empty() {
            formatter.output.push_str("()");
            return;
        }

        if self.arguments.iter().all(CallArgument::is_inline_friendly) {
            formatter.output.push('(');

            let mut argument_iterator = self.arguments.iter().peekable();
            while let Some(call_argument) = argument_iterator.next() {
                call_argument.push_to_formatter(formatter, ExpressionFormat::Inline);

                if argument_iterator.peek().is_some() {
                    formatter.output.push_str(", ");
                }
            }

            formatter.output.push(')');
            return;
        }

        formatter.output.push('(');
        formatter.push_newline();
        formatter.indentation_depth += 1;

        for call_argument in &self.arguments {
            formatter.push_indent();
            call_argument.push_to_formatter(formatter, ExpressionFormat::Canonical);
            formatter.output.push(',');
            formatter.push_newline();
        }

        formatter.indentation_depth -= 1;
        formatter.push_indent();
        formatter.output.push(')');
    }
}

impl CallArgument {
    fn push_to_formatter(&self, formatter: &mut DslFormatter, expression_format: ExpressionFormat) {
        match self {
            Self::Positional(expression) => expression.push_to_formatter(formatter, expression_format),
            Self::Named(named_argument) => {
                formatter.output.push_str(&named_argument.name);
                formatter.output.push_str(": ");
                named_argument.value.push_to_formatter(formatter, expression_format);
            }
        }
    }

    fn is_inline_friendly(&self) -> bool {
        match self {
            Self::Positional(expression) => expression.is_inline_friendly(),
            Self::Named(named_argument) => named_argument.value.is_inline_friendly(),
        }
    }
}

fn render_string_literal(raw_string: &str) -> String {
    if raw_string.contains('\n') {
        return format!("\"\"\"{}\"\"\"", escape_multiline_string_text(raw_string));
    }

    format!("\"{}\"", escape_quoted_string_text(raw_string))
}

fn escape_quoted_string_text(raw_string: &str) -> String {
    let mut escaped = String::new();

    for character in raw_string.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '{' => escaped.push_str("\\{"),
            '}' => escaped.push_str("\\}"),
            _ => escaped.push(character),
        }
    }

    escaped
}

fn escape_multiline_string_text(raw_string: &str) -> String {
    let mut escaped = String::new();

    for character in raw_string.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '{' => escaped.push_str("\\{"),
            '}' => escaped.push_str("\\}"),
            _ => escaped.push(character),
        }
    }

    escaped.replace("\"\"\"", "\\\"\\\"\\\"")
}

#[cfg(test)]
mod tests {
    use super::format_workflow_source;

    #[test]
    fn formatter_is_idempotent() {
        let unformatted_workflow = r#"
provider openai   {driver:"openai" endpoint:"https://api.openai.com/v1"  models:["gpt-4o-mini","gpt-4o",]}

input {topic:string tags:[string]}

schema Insight { metadata:{source:string rank:number}|null pair:(string,number)}

agent planner {model:openai("gpt-4o-mini") prompt:"Plan for {{input.topic}}" context:{tags:input.tags nested:{level:2}} tools:[tool.web_search,tool.issue_tracker_lookup(project:"engine-ai",status:"open"),] output:schema.Insight}

agent summaries for item in input.tags {model:openai("gpt-4o-mini") prompt:"""Summary for {{item}}\nGenerated for {{ input.topic }}""" output:string}

output {plan:agent.planner summaries:agent.summaries}
"#;

        let first_pass = format_workflow_source(unformatted_workflow).expect("first formatting pass should succeed");
        let second_pass = format_workflow_source(&first_pass).expect("second formatting pass should succeed");

        assert_eq!(first_pass, second_pass);
    }

    #[test]
    fn formatter_matches_golden_output_for_representative_syntax() {
        let unformatted_workflow = r#"
provider openai   {driver:"openai" endpoint:"https://api.openai.com/v1"  models:["gpt-4o-mini","gpt-4o",]}

input {topic:string tags:[string]}

schema Insight { metadata:{source:string rank:number}|null pair:(string,number)}

agent planner {model:openai("gpt-4o-mini") prompt:"Plan for {{input.topic}}" context:{tags:input.tags nested:{level:2}} tools:[tool.web_search,tool.issue_tracker_lookup(project:"engine-ai",status:"open"),] output:schema.Insight}

agent summaries for item in input.tags {model:openai("gpt-4o-mini") prompt:"""Summary for {{item}}\nGenerated for {{ input.topic }}""" output:string}

output {plan:agent.planner summaries:agent.summaries}
"#;

        let formatted = format_workflow_source(unformatted_workflow).expect("formatting should succeed");

        let expected = r#"provider openai {
    driver: "openai"
    endpoint: "https://api.openai.com/v1"
    models: [
        "gpt-4o-mini",
        "gpt-4o",
    ]
}

input {
    topic: string
    tags: [string]
}

schema Insight {
    metadata: {
        source: string
        rank: number
    } | null
    pair: (string, number)
}

agent planner {
    model: openai("gpt-4o-mini")
    prompt: "Plan for {{ input.topic }}"
    context: {
        tags: input.tags
        nested: {
            level: 2
        }
    }
    tools: [
        tool.web_search,
        tool.issue_tracker_lookup(project: "engine-ai", status: "open"),
    ]
    output: schema.Insight
}

agent summaries for item in input.tags {
    model: openai("gpt-4o-mini")
    prompt: """Summary for {{ item }}
Generated for {{ input.topic }}"""
    output: string
}

output {
    plan: agent.planner
    summaries: agent.summaries
}
"#;

        assert_eq!(formatted, expected);
    }

    #[test]
    fn formatter_rejects_line_comments() {
        let workflow_with_comment = r#"
provider openai {
    driver: "openai" // provider driver
}
"#;

        let format_result = format_workflow_source(workflow_with_comment);

        assert!(format_result.is_err());

        let error_message = format_result
            .err()
            .map(|format_error| format_error.to_string())
            .expect("formatter should return an error");

        assert!(error_message.contains("formatting rejects line comments"));
    }
}
