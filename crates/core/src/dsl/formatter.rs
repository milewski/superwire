use std::collections::HashMap;

use thiserror::Error;

use super::ast::{
    AgentDeclaration, AgentProperty, AgentPropertyName, CallArgument, Declaration, DeclarationKeyword, Expression, ForClauseKeyword,
    FunctionCall, ObjectField, Reference, StringTemplate, StringTemplatePart, TypeExpression, TypedField, Workflow,
};
use super::parse_workflow;
use super::parser::DslParseError;

const MAX_LINE_WIDTH: usize = 120;

#[derive(Debug, Error)]
pub enum DslFormatError {
    #[error("failed to parse DSL while formatting: {0}")]
    Parse(#[from] DslParseError),
}

pub fn format_workflow_source(source_text: &str) -> Result<String, DslFormatError> {
    let workflow = parse_workflow(source_text)?;
    let mut formatter = DslFormatter::new();
    formatter.push_workflow(&workflow);

    let formatted_without_comments = formatter.finish();

    Ok(CommentPreserver::new(source_text, formatted_without_comments).with_preserved_comments())
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

    fn finish(mut self) -> String {
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }

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

    fn inline_expression(&self, expression: &Expression) -> String {
        let mut inline_formatter = DslFormatter::new();
        expression.push_to_formatter(&mut inline_formatter, ExpressionFormat::Inline);
        inline_formatter.output
    }

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

    fn push_multiline_string_block(&mut self, escaped_multiline_contents: &str) {
        let normalized_multiline_lines = Self::normalize_multiline_string_lines(escaped_multiline_contents);

        self.output.push_str("\"\"\"");
        self.push_newline();
        self.indentation_depth += 1;

        for multiline_content_line in normalized_multiline_lines {
            self.push_indent();
            self.output.push_str(&multiline_content_line);
            self.push_newline();
        }

        self.indentation_depth -= 1;
        self.push_indent();
        self.output.push_str("\"\"\"");
    }

    fn push_multiline_string_block_from_lines(&mut self, multiline_content_lines: &[String]) {
        self.output.push_str("\"\"\"");
        self.push_newline();
        self.indentation_depth += 1;

        for multiline_content_line in multiline_content_lines {
            self.push_indent();
            self.output.push_str(multiline_content_line);
            self.push_newline();
        }

        self.indentation_depth -= 1;
        self.push_indent();
        self.output.push_str("\"\"\"");
    }

    fn can_fit_inline_text(&self, inline_text: &str) -> bool {
        !inline_text.contains('\n') && self.current_line_width() + inline_text.chars().count() <= MAX_LINE_WIDTH
    }

    fn current_line_width(&self) -> usize {
        self.output.rsplit('\n').next().map_or(0, |line_text| line_text.chars().count())
    }

    fn wrap_multiline_string_value(&self, raw_string: &str) -> Vec<String> {
        let content_width_limit = MAX_LINE_WIDTH.saturating_sub((self.indentation_depth + 1) * 4);
        let effective_width_limit = content_width_limit.max(20);

        let mut wrapped_lines = Vec::new();
        let mut remaining_text = raw_string.trim().to_owned();

        while remaining_text.chars().count() > effective_width_limit {
            let split_character_index = find_wrap_split_index(&remaining_text, effective_width_limit)
                .unwrap_or_else(|| effective_width_limit.min(remaining_text.chars().count()));

            let mut current_line = remaining_text.chars().take(split_character_index).collect::<String>();
            current_line = current_line.trim_end().to_owned();

            if current_line.is_empty() {
                break;
            }

            wrapped_lines.push(escape_multiline_string_text(&current_line));

            let wrapped_remainder = remaining_text
                .chars()
                .skip(split_character_index)
                .collect::<String>()
                .trim_start()
                .to_owned();

            wrapped_remainder.clone_into(&mut remaining_text);
        }

        wrapped_lines.push(escape_multiline_string_text(&remaining_text));
        wrapped_lines
    }

    fn prompt_line_exceeds_width_for_string_literal(&self, string_value: &str) -> bool {
        let quoted_string_literal = render_expression_string_literal(string_value);
        let projected_line_width =
            self.indentation_depth * 4 + AgentPropertyName::Prompt.as_str().chars().count() + 2 + quoted_string_literal.chars().count();

        projected_line_width > MAX_LINE_WIDTH
    }

    fn normalize_multiline_string_lines(multiline_contents: &str) -> Vec<String> {
        let mut content_lines = multiline_contents.split('\n').map(ToOwned::to_owned).collect::<Vec<_>>();

        while content_lines.first().is_some_and(|line_text| line_text.trim().is_empty()) {
            let _ = content_lines.remove(0);
        }

        while content_lines.last().is_some_and(|line_text| line_text.trim().is_empty()) {
            let _ = content_lines.pop();
        }

        if content_lines.is_empty() {
            return content_lines;
        }

        let minimum_indentation = content_lines
            .iter()
            .filter(|line_text| !line_text.trim().is_empty())
            .map(|line_text| line_text.chars().take_while(|character| character.is_whitespace()).count())
            .min()
            .unwrap_or(0);

        content_lines
            .into_iter()
            .map(|line_text| {
                if line_text.trim().is_empty() {
                    return String::new();
                }

                line_text.chars().skip(minimum_indentation).collect::<String>()
            })
            .collect::<Vec<_>>()
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
        let mut declaration_header = format!("{} {}", DeclarationKeyword::Agent.as_str(), self.name);

        if let Some(loop_declaration) = &self.for_loop {
            declaration_header.push(' ');
            declaration_header.push_str(ForClauseKeyword::For.as_str());
            declaration_header.push(' ');
            declaration_header.push_str(&loop_declaration.iterator_name);
            declaration_header.push(' ');
            declaration_header.push_str(ForClauseKeyword::In.as_str());
            declaration_header.push(' ');
            declaration_header.push_str(&formatter.inline_expression(&loop_declaration.iterable));
        }

        formatter.push_declaration_block_start(&declaration_header);

        let mut property_iterator = self.properties.iter().peekable();

        while let Some(agent_property) = property_iterator.next() {
            let should_insert_trailing_visual_separator = agent_property.should_have_trailing_visual_separator(formatter);
            agent_property.push_to_formatter(formatter);

            if property_iterator.peek().is_some() && should_insert_trailing_visual_separator {
                formatter.push_newline();
            }
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

    fn should_have_trailing_visual_separator(&self, formatter: &DslFormatter) -> bool {
        match self {
            Self::Prompt(expression) => expression.is_multiline_prompt_expression(formatter),
            Self::Model(_)
            | Self::Output(_)
            | Self::Context(_)
            | Self::Inference(_)
            | Self::Tools(_)
            | Self::Custom { name: _, value: _ } => false,
        }
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
            formatter.output.push_str(&render_plain_string_literal(description));
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
            Self::StringEnum(enum_value) => formatter.output.push_str(&render_plain_string_literal(enum_value)),
            Self::StringEnumReference(reference) => reference.push_to_formatter(formatter),
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
                let mut tuple_item_iterator = tuple_items.iter().peekable();

                while let Some(tuple_item) = tuple_item_iterator.next() {
                    tuple_item.push_to_formatter(formatter);

                    if tuple_item_iterator.peek().is_some() {
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
            Self::StringLiteral(string_value) => {
                if string_value.contains('\n') {
                    formatter.push_multiline_string_block(&escape_multiline_string_text(string_value));
                } else if expression_format == ExpressionFormat::Canonical {
                    let quoted_string_literal = render_expression_string_literal(string_value);

                    if formatter.can_fit_inline_text(&quoted_string_literal) {
                        formatter.output.push_str(&quoted_string_literal);
                    } else {
                        let wrapped_multiline_lines = formatter.wrap_multiline_string_value(string_value);
                        formatter.push_multiline_string_block_from_lines(&wrapped_multiline_lines);
                    }
                } else {
                    formatter.output.push_str(&render_expression_string_literal(string_value));
                }
            }
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

                    let mut array_item_iterator = array_items.iter().peekable();
                    while let Some(array_item) = array_item_iterator.next() {
                        array_item.push_to_formatter(formatter, ExpressionFormat::Inline);

                        if array_item_iterator.peek().is_some() {
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

                if let Some(inline_array_literal) = self.inline_array_literal(formatter) {
                    if formatter.can_fit_inline_text(&inline_array_literal) {
                        formatter.output.push_str(&inline_array_literal);
                        return;
                    }
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

                    let mut object_field_iterator = object_fields.iter().peekable();
                    while let Some(object_field) = object_field_iterator.next() {
                        formatter.output.push_str(&object_field.name);
                        formatter.output.push_str(": ");
                        object_field.value.push_to_formatter(formatter, ExpressionFormat::Inline);

                        if object_field_iterator.peek().is_some() {
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

    fn inline_array_literal(&self, formatter: &DslFormatter) -> Option<String> {
        let Self::ArrayLiteral(array_items) = self else {
            return None;
        };

        if array_items.iter().any(|array_item| !array_item.is_inline_friendly()) {
            return None;
        }

        let mut inline_array_literal = String::from("[");
        let mut array_item_iterator = array_items.iter().peekable();

        while let Some(array_item) = array_item_iterator.next() {
            inline_array_literal.push_str(&formatter.inline_expression(array_item));

            if array_item_iterator.peek().is_some() {
                inline_array_literal.push_str(", ");
            }
        }

        inline_array_literal.push(']');
        Some(inline_array_literal)
    }

    fn is_multiline_prompt_expression(&self, formatter: &DslFormatter) -> bool {
        match self {
            Self::StringLiteral(string_value) => {
                string_value.contains('\n') || formatter.prompt_line_exceeds_width_for_string_literal(string_value)
            }
            Self::StringTemplate(string_template) => string_template.is_multiline(),
            Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::Reference(_)
            | Self::FunctionCall(_)
            | Self::ArrayLiteral(_)
            | Self::ObjectLiteral(_) => false,
        }
    }
}

impl StringTemplate {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        let is_multiline = self.is_multiline();

        if is_multiline {
            formatter.push_multiline_string_block(&self.render_multiline_contents(formatter));
            return;
        }

        formatter.output.push('"');

        for string_template_part in &self.parts {
            match string_template_part {
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

    fn render_multiline_contents(&self, formatter: &DslFormatter) -> String {
        let mut rendered_contents = String::new();

        for string_template_part in &self.parts {
            match string_template_part {
                StringTemplatePart::Text(text) => rendered_contents.push_str(&escape_multiline_string_text(text)),
                StringTemplatePart::Interpolation(expression) => {
                    rendered_contents.push_str("{{ ");
                    rendered_contents.push_str(&formatter.inline_expression(expression));
                    rendered_contents.push_str(" }}");
                }
            }
        }

        rendered_contents
    }

    fn is_multiline(&self) -> bool {
        self.parts
            .iter()
            .any(|string_template_part| matches!(string_template_part, StringTemplatePart::Text(text) if text.contains('\n')))
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
            let inline_arguments = self.inline_argument_list(formatter);
            let inline_call_suffix = format!("({inline_arguments})");

            if formatter.can_fit_inline_text(&inline_call_suffix) {
                formatter.output.push_str(&inline_call_suffix);
                return;
            }
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

    fn inline_argument_list(&self, formatter: &DslFormatter) -> String {
        let mut inline_argument_list = String::new();
        let mut argument_iterator = self.arguments.iter().peekable();

        while let Some(call_argument) = argument_iterator.next() {
            inline_argument_list.push_str(&call_argument.render_inline(formatter));

            if argument_iterator.peek().is_some() {
                inline_argument_list.push_str(", ");
            }
        }

        inline_argument_list
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

    fn render_inline(&self, formatter: &DslFormatter) -> String {
        match self {
            Self::Positional(expression) => formatter.inline_expression(expression),
            Self::Named(named_argument) => {
                format!("{}: {}", named_argument.name, formatter.inline_expression(&named_argument.value))
            }
        }
    }
}

fn find_wrap_split_index(text: &str, width_limit: usize) -> Option<usize> {
    let mut last_whitespace_character_index = None;

    for (character_count, character) in text.chars().enumerate() {
        if character_count >= width_limit {
            break;
        }

        if character.is_whitespace() {
            last_whitespace_character_index = Some(character_count);
        }
    }

    last_whitespace_character_index
}

fn render_expression_string_literal(raw_string: &str) -> String {
    if raw_string.contains('\n') {
        return format!("\"\"\"{}\"\"\"", escape_multiline_string_text(raw_string));
    }

    format!("\"{}\"", escape_quoted_string_text(raw_string))
}

fn render_plain_string_literal(raw_string: &str) -> String {
    if raw_string.contains('\n') {
        return format!("\"\"\"{}\"\"\"", escape_multiline_plain_string_text(raw_string));
    }

    format!("\"{}\"", escape_plain_string_text(raw_string))
}

fn escape_quoted_string_text(raw_string: &str) -> String {
    let mut escaped_string = String::new();

    for character in raw_string.chars() {
        match character {
            '\\' => escaped_string.push_str("\\\\"),
            '"' => escaped_string.push_str("\\\""),
            '\n' => escaped_string.push_str("\\n"),
            '\r' => escaped_string.push_str("\\r"),
            '\t' => escaped_string.push_str("\\t"),
            '{' => escaped_string.push_str("\\{"),
            '}' => escaped_string.push_str("\\}"),
            _ => escaped_string.push(character),
        }
    }

    escaped_string
}

fn escape_plain_string_text(raw_string: &str) -> String {
    let mut escaped_string = String::new();

    for character in raw_string.chars() {
        match character {
            '\\' => escaped_string.push_str("\\\\"),
            '"' => escaped_string.push_str("\\\""),
            '\n' => escaped_string.push_str("\\n"),
            '\r' => escaped_string.push_str("\\r"),
            '\t' => escaped_string.push_str("\\t"),
            _ => escaped_string.push(character),
        }
    }

    escaped_string
}

fn escape_multiline_string_text(raw_string: &str) -> String {
    let mut escaped_string = String::new();

    for character in raw_string.chars() {
        match character {
            '\\' => escaped_string.push_str("\\\\"),
            '{' => escaped_string.push_str("\\{"),
            '}' => escaped_string.push_str("\\}"),
            _ => escaped_string.push(character),
        }
    }

    escaped_string.replace("\"\"\"", "\\\"\\\"\\\"")
}

fn escape_multiline_plain_string_text(raw_string: &str) -> String {
    raw_string.replace("\"\"\"", "\\\"\\\"\\\"")
}

struct CommentPreserver<'source> {
    source_text: &'source str,
    formatted_without_comments: String,
}

impl<'source> CommentPreserver<'source> {
    fn new(source_text: &'source str, formatted_without_comments: String) -> Self {
        Self {
            source_text,
            formatted_without_comments,
        }
    }

    fn with_preserved_comments(self) -> String {
        let source_line_analyses = SourceLineAnalyzer::new(self.source_text).analyze();

        if !source_line_analyses.iter().any(SourceLineAnalysis::has_comment) {
            return self.formatted_without_comments;
        }

        let mut formatted_lines = self.formatted_without_comments.lines().map(ToOwned::to_owned).collect::<Vec<_>>();

        let source_code_signature_lines = SourceCodeSignatureLine::collect(&source_line_analyses);
        let formatted_code_signature_lines = FormattedCodeSignatureLine::collect(&formatted_lines);
        let source_to_formatted_map = map_source_lines_to_formatted_lines(&source_code_signature_lines, &formatted_code_signature_lines);

        apply_inline_comments(&source_line_analyses, &source_to_formatted_map, &mut formatted_lines);
        apply_standalone_comments(&source_line_analyses, &source_to_formatted_map, &mut formatted_lines);

        let mut formatted_with_comments = formatted_lines.join("\n");

        if self.formatted_without_comments.ends_with('\n') {
            formatted_with_comments.push('\n');
        }

        formatted_with_comments
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommentKind {
    Inline,
    Standalone,
}

#[derive(Clone, Debug)]
struct CommentFragment {
    text: String,
    comment_kind: CommentKind,
}

#[derive(Clone, Debug)]
struct SourceLineAnalysis {
    line_number: usize,
    code_text: String,
    comment: Option<CommentFragment>,
    is_within_multiline_string: bool,
}

impl SourceLineAnalysis {
    fn has_comment(&self) -> bool {
        self.comment.is_some()
    }

    fn code_signature(&self) -> Option<String> {
        if self.is_within_multiline_string {
            return None;
        }

        line_signature(&self.code_text)
    }

    fn is_blank_line(&self) -> bool {
        self.comment.is_none() && self.code_text.trim().is_empty()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StringScanState {
    Normal,
    QuotedString,
    MultilineString,
}

struct SourceLineAnalyzer<'source> {
    source_text: &'source str,
}

impl<'source> SourceLineAnalyzer<'source> {
    fn new(source_text: &'source str) -> Self {
        Self { source_text }
    }

    fn analyze(&self) -> Vec<SourceLineAnalysis> {
        let mut source_line_analyses = Vec::new();
        let mut string_scan_state = StringScanState::Normal;

        for (line_index, source_line) in self.source_text.lines().enumerate() {
            let starts_inside_multiline_string = string_scan_state == StringScanState::MultilineString;
            let comment_start_byte_index = find_comment_start_byte_index(source_line, &mut string_scan_state);

            let (code_text, comment) = if let Some(comment_start) = comment_start_byte_index {
                let code_text = source_line[..comment_start].to_owned();
                let comment_text = source_line[comment_start..].to_owned();
                let comment_kind = if code_text.trim().is_empty() {
                    CommentKind::Standalone
                } else {
                    CommentKind::Inline
                };

                (
                    code_text,
                    Some(CommentFragment {
                        text: comment_text,
                        comment_kind,
                    }),
                )
            } else {
                (source_line.to_owned(), None)
            };

            source_line_analyses.push(SourceLineAnalysis {
                line_number: line_index + 1,
                code_text,
                comment,
                is_within_multiline_string: starts_inside_multiline_string,
            });
        }

        source_line_analyses
    }
}

fn find_comment_start_byte_index(source_line: &str, string_scan_state: &mut StringScanState) -> Option<usize> {
    let mut byte_index = 0;

    while byte_index < source_line.len() {
        let remaining_source = &source_line[byte_index..];

        if *string_scan_state == StringScanState::Normal && remaining_source.starts_with("\"\"\"") {
            *string_scan_state = StringScanState::MultilineString;
            byte_index += 3;
            continue;
        }

        if *string_scan_state == StringScanState::MultilineString && remaining_source.starts_with("\"\"\"") {
            *string_scan_state = StringScanState::Normal;
            byte_index += 3;
            continue;
        }

        if *string_scan_state == StringScanState::Normal && remaining_source.starts_with("//") {
            return Some(byte_index);
        }

        let current_character = remaining_source
            .chars()
            .next()
            .expect("remaining source should include a character");

        match string_scan_state {
            StringScanState::Normal => {
                if current_character == '"' {
                    *string_scan_state = StringScanState::QuotedString;
                }
            }
            StringScanState::QuotedString => {
                if current_character == '\\' {
                    byte_index += current_character.len_utf8();

                    if byte_index < source_line.len() {
                        let escaped_character = source_line[byte_index..].chars().next().expect("escaped character should exist");

                        byte_index += escaped_character.len_utf8();
                    }

                    continue;
                }

                if current_character == '"' {
                    *string_scan_state = StringScanState::Normal;
                }
            }
            StringScanState::MultilineString => {}
        }

        byte_index += current_character.len_utf8();
    }

    if *string_scan_state == StringScanState::QuotedString {
        *string_scan_state = StringScanState::Normal;
    }

    None
}

#[derive(Clone, Debug)]
struct SourceCodeSignatureLine {
    source_line_number: usize,
    signature: String,
}

impl SourceCodeSignatureLine {
    fn collect(source_line_analyses: &[SourceLineAnalysis]) -> Vec<Self> {
        let mut source_code_signature_lines = Vec::new();

        for source_line_analysis in source_line_analyses {
            let Some(signature) = source_line_analysis.code_signature() else {
                continue;
            };

            source_code_signature_lines.push(Self {
                source_line_number: source_line_analysis.line_number,
                signature,
            });
        }

        source_code_signature_lines
    }
}

#[derive(Clone, Debug)]
struct FormattedCodeSignatureLine {
    formatted_line_index: usize,
    signature: String,
}

impl FormattedCodeSignatureLine {
    fn collect(formatted_lines: &[String]) -> Vec<Self> {
        let mut formatted_code_signature_lines = Vec::new();
        let mut is_inside_multiline_string = false;

        for (line_index, line_text) in formatted_lines.iter().enumerate() {
            let is_current_line_within_multiline = is_inside_multiline_string;
            is_inside_multiline_string = update_multiline_string_state(is_inside_multiline_string, line_text);

            if is_current_line_within_multiline || line_text.trim() == "\"\"\"" {
                continue;
            }

            let Some(signature) = line_signature(line_text) else {
                continue;
            };

            formatted_code_signature_lines.push(Self {
                formatted_line_index: line_index,
                signature,
            });
        }

        formatted_code_signature_lines
    }
}

fn line_signature(line_text: &str) -> Option<String> {
    let signature = line_text.chars().filter(|character| !character.is_whitespace()).collect::<String>();

    if signature.is_empty() {
        return None;
    }

    Some(signature)
}

fn map_source_lines_to_formatted_lines(
    source_code_signature_lines: &[SourceCodeSignatureLine],
    formatted_code_signature_lines: &[FormattedCodeSignatureLine],
) -> HashMap<usize, usize> {
    let mut source_to_formatted_map = HashMap::new();
    let mut formatted_cursor = 0_usize;

    for source_code_signature_line in source_code_signature_lines {
        while formatted_cursor < formatted_code_signature_lines.len() {
            let formatted_code_signature_line = &formatted_code_signature_lines[formatted_cursor];

            if formatted_code_signature_line.signature == source_code_signature_line.signature {
                source_to_formatted_map.insert(
                    source_code_signature_line.source_line_number,
                    formatted_code_signature_line.formatted_line_index,
                );

                formatted_cursor += 1;
                break;
            }

            formatted_cursor += 1;
        }
    }

    source_to_formatted_map
}

fn apply_inline_comments(
    source_line_analyses: &[SourceLineAnalysis],
    source_to_formatted_map: &HashMap<usize, usize>,
    formatted_lines: &mut [String],
) {
    for source_line_analysis in source_line_analyses {
        let Some(comment) = &source_line_analysis.comment else {
            continue;
        };

        if comment.comment_kind != CommentKind::Inline {
            continue;
        }

        let Some(formatted_line_index) = source_to_formatted_map.get(&source_line_analysis.line_number) else {
            continue;
        };

        let Some(formatted_line) = formatted_lines.get_mut(*formatted_line_index) else {
            continue;
        };

        if formatted_line.trim().is_empty() {
            comment.text.trim_start().clone_into(formatted_line);
            continue;
        }

        formatted_line.push(' ');
        formatted_line.push_str(comment.text.trim_start());
    }
}

#[derive(Clone, Debug)]
struct StandaloneCommentInsertion {
    source_line_number: usize,
    target_formatted_line_index: usize,
    insert_after_target: bool,
    preserve_blank_line_before: bool,
    preserve_blank_line_after: bool,
    comment_text: String,
}

fn apply_standalone_comments(
    source_line_analyses: &[SourceLineAnalysis],
    source_to_formatted_map: &HashMap<usize, usize>,
    formatted_lines: &mut Vec<String>,
) {
    let mut standalone_comment_insertions = Vec::new();
    let source_line_count = source_line_analyses.len();

    for (analysis_index, source_line_analysis) in source_line_analyses.iter().enumerate() {
        let Some(comment) = &source_line_analysis.comment else {
            continue;
        };

        if comment.comment_kind != CommentKind::Standalone {
            continue;
        }

        let next_mapped_line =
            find_next_mapped_formatted_line(source_line_analysis.line_number, source_line_count, source_to_formatted_map);
        let previous_mapped_line = find_previous_mapped_formatted_line(source_line_analysis.line_number, source_to_formatted_map);

        let (target_formatted_line_index, insert_after_target) = if let Some(next_line) = next_mapped_line {
            (next_line, false)
        } else if let Some(previous_line) = previous_mapped_line {
            if let Some(next_non_empty_line) =
                find_first_non_empty_formatted_line_outside_multiline_strings_after(previous_line, formatted_lines)
            {
                (next_non_empty_line, false)
            } else {
                (previous_line, true)
            }
        } else {
            (0, false)
        };

        let indentation_source_line = formatted_lines.get(target_formatted_line_index);

        let indentation = indentation_source_line
            .map(|line_text| leading_whitespace(line_text.as_str()))
            .unwrap_or_default();
        let preserve_blank_line_before = source_line_analyses
            .get(analysis_index.saturating_sub(1))
            .is_some_and(SourceLineAnalysis::is_blank_line);
        let preserve_blank_line_after = source_line_analyses
            .get(analysis_index + 1)
            .is_some_and(SourceLineAnalysis::is_blank_line);

        standalone_comment_insertions.push(StandaloneCommentInsertion {
            source_line_number: source_line_analysis.line_number,
            target_formatted_line_index,
            insert_after_target,
            preserve_blank_line_before,
            preserve_blank_line_after,
            comment_text: format!("{indentation}{}", comment.text.trim_start()),
        });
    }

    standalone_comment_insertions.sort_by_key(|comment_insertion| {
        (
            comment_insertion.target_formatted_line_index,
            comment_insertion.insert_after_target,
            comment_insertion.source_line_number,
        )
    });

    let mut insertion_offset = 0_usize;

    for standalone_comment_insertion in standalone_comment_insertions {
        let base_insertion_index = if standalone_comment_insertion.insert_after_target {
            standalone_comment_insertion.target_formatted_line_index.saturating_add(1)
        } else {
            standalone_comment_insertion.target_formatted_line_index
        };

        let mut insertion_index = base_insertion_index.saturating_add(insertion_offset).min(formatted_lines.len());

        let should_preserve_or_insert_blank_line_before = standalone_comment_insertion.preserve_blank_line_before
            || should_insert_visual_separator_before_comment(insertion_index, formatted_lines);

        if should_preserve_or_insert_blank_line_before && !has_blank_line_before_index(insertion_index, formatted_lines) {
            formatted_lines.insert(insertion_index, String::new());
            insertion_offset += 1;
            insertion_index += 1;
        }

        formatted_lines.insert(insertion_index, standalone_comment_insertion.comment_text);
        insertion_offset += 1;
        insertion_index += 1;

        if standalone_comment_insertion.preserve_blank_line_after && !has_blank_line_at_index(insertion_index, formatted_lines) {
            formatted_lines.insert(insertion_index, String::new());
            insertion_offset += 1;
        }
    }
}

fn has_blank_line_before_index(insertion_index: usize, formatted_lines: &[String]) -> bool {
    if insertion_index == 0 {
        return false;
    }

    formatted_lines
        .get(insertion_index.saturating_sub(1))
        .is_some_and(|line_text| line_text.trim().is_empty())
}

fn has_blank_line_at_index(insertion_index: usize, formatted_lines: &[String]) -> bool {
    formatted_lines
        .get(insertion_index)
        .is_some_and(|line_text| line_text.trim().is_empty())
}

fn should_insert_visual_separator_before_comment(insertion_index: usize, formatted_lines: &[String]) -> bool {
    let mut previous_line_index = insertion_index;

    while previous_line_index > 0 {
        previous_line_index = previous_line_index.saturating_sub(1);

        let Some(previous_line_text) = formatted_lines.get(previous_line_index) else {
            continue;
        };

        if previous_line_text.trim().is_empty() {
            continue;
        }

        let previous_line_without_indent = previous_line_text.trim_start();

        if previous_line_without_indent.starts_with("//") {
            return false;
        }

        let previous_line_without_trailing_whitespace = previous_line_text.trim_end();

        if previous_line_without_trailing_whitespace.ends_with('{') || previous_line_without_trailing_whitespace.ends_with('[') {
            return false;
        }

        return true;
    }

    false
}

fn find_next_mapped_formatted_line(
    source_line_number: usize,
    source_line_count: usize,
    source_to_formatted_map: &HashMap<usize, usize>,
) -> Option<usize> {
    for line_number in source_line_number + 1..=source_line_count {
        let Some(formatted_line_index) = source_to_formatted_map.get(&line_number) else {
            continue;
        };

        return Some(*formatted_line_index);
    }

    None
}

fn find_previous_mapped_formatted_line(source_line_number: usize, source_to_formatted_map: &HashMap<usize, usize>) -> Option<usize> {
    if source_line_number <= 1 {
        return None;
    }

    for line_number in (1..source_line_number).rev() {
        let Some(formatted_line_index) = source_to_formatted_map.get(&line_number) else {
            continue;
        };

        return Some(*formatted_line_index);
    }

    None
}

fn find_first_non_empty_formatted_line_outside_multiline_strings_after(
    start_line_index: usize,
    formatted_lines: &[String],
) -> Option<usize> {
    let mut is_inside_multiline_string = false;

    for line_text in formatted_lines.iter().take(start_line_index.saturating_add(1)) {
        is_inside_multiline_string = update_multiline_string_state(is_inside_multiline_string, line_text);
    }

    let first_candidate_index = start_line_index.saturating_add(1);

    for line_index in first_candidate_index..formatted_lines.len() {
        let Some(line_text) = formatted_lines.get(line_index) else {
            continue;
        };

        is_inside_multiline_string = update_multiline_string_state(is_inside_multiline_string, line_text);

        if is_inside_multiline_string || line_text.trim() == "\"\"\"" {
            continue;
        }

        if line_text.trim().is_empty() {
            continue;
        }

        return Some(line_index);
    }

    None
}

fn update_multiline_string_state(current_state: bool, line_text: &str) -> bool {
    let triple_quote_occurrences = line_text.matches("\"\"\"").count();

    if triple_quote_occurrences.is_multiple_of(2) {
        return current_state;
    }

    !current_state
}

fn leading_whitespace(line_text: &str) -> String {
    line_text
        .chars()
        .take_while(|character| character.is_whitespace())
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::format_workflow_source;
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn formatter_is_idempotent_for_all_workflow_examples() {
        for workflow_path in discover_workflow_examples() {
            let workflow_source = fs::read_to_string(&workflow_path)
                .unwrap_or_else(|read_error| panic!("failed to read {}: {read_error}", workflow_path.display()));

            let first_formatted_output = format_workflow_source(&workflow_source)
                .unwrap_or_else(|format_error| panic!("failed to format {}: {format_error}", workflow_path.display()));

            let second_formatted_output = format_workflow_source(&first_formatted_output)
                .unwrap_or_else(|format_error| panic!("failed to re-format {}: {format_error}", workflow_path.display()));

            assert_eq!(
                first_formatted_output,
                second_formatted_output,
                "formatter output should be stable for {}",
                workflow_path.display()
            );
        }
    }

    #[test]
    fn formatter_matches_expected_output_for_representative_source() {
        let source_text = "provider openai   {driver:\"openai\" models:[\"gpt-4o-mini\",]}\n\noutput { result: \"ok\" }\n";

        let expected_output =
            "provider openai {\n    driver: \"openai\"\n    models: [\"gpt-4o-mini\"]\n}\n\noutput {\n    result: \"ok\"\n}\n";

        let formatted_source = format_workflow_source(source_text).expect("representative workflow should format successfully");

        assert_eq!(formatted_source, expected_output);
    }

    #[test]
    fn formatter_places_standalone_comment_before_next_declaration_when_source_is_single_line_block() {
        let source_text =
            "// provider declaration\nprovider openai {\n// provider driver\n    driver:\"openai\" // inline driver comment\n}\n\n// output heading\noutput { value: \"ok\" }\n";

        let expected_output =
            "// provider declaration\nprovider openai {\n    // provider driver\n    driver: \"openai\" // inline driver comment\n}\n\n// output heading\noutput {\n    value: \"ok\"\n}\n";

        let formatted_source = format_workflow_source(source_text).expect("workflow with standalone comment should format successfully");

        assert_eq!(formatted_source, expected_output);
    }

    fn discover_workflow_examples() -> Vec<PathBuf> {
        let workflows_directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("workflows");
        let mut workflow_paths = Vec::new();

        collect_paths_by_extension(&workflows_directory, "ai", &mut workflow_paths);
        workflow_paths.sort();

        workflow_paths
    }

    fn collect_paths_by_extension(current_directory: &Path, extension: &str, collected_paths: &mut Vec<PathBuf>) {
        let directory_entries = fs::read_dir(current_directory)
            .unwrap_or_else(|read_error| panic!("failed to read directory {}: {read_error}", current_directory.display()));

        for directory_entry_result in directory_entries {
            let directory_entry = directory_entry_result
                .unwrap_or_else(|read_error| panic!("failed to read entry in {}: {read_error}", current_directory.display()));

            let entry_path = directory_entry.path();

            if entry_path.is_dir() {
                collect_paths_by_extension(&entry_path, extension, collected_paths);

                continue;
            }

            if entry_path.extension().and_then(|path_extension| path_extension.to_str()) != Some(extension) {
                continue;
            }

            collected_paths.push(entry_path);
        }
    }
}
