use crate::ast::{Asset, CallArgument, Expression, FunctionCall, MatchBranch, ObjectField, Reference, StringTemplate, StringTemplatePart};

use super::wrapping::{
    escape_multiline_string_text, escape_quoted_string_text, render_expression_string_literal, render_object_field_name,
};
use super::DslFormatter;

impl DslFormatter {
    pub(super) fn inline_expression(&self, expression: &Expression) -> String {
        let mut inline_formatter = DslFormatter::new();
        expression.push_to_formatter(&mut inline_formatter, ExpressionFormat::Inline);
        inline_formatter.output
    }
}

impl ObjectField {
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        formatter.push_indent();
        formatter.output.push_str(&render_object_field_name(&self.name));
        formatter.output.push_str(": ");
        self.value.push_to_formatter(formatter, ExpressionFormat::Canonical);
        formatter.push_newline();
    }

    pub(super) fn push_config_property_to_formatter(&self, formatter: &mut DslFormatter) {
        let Expression::ObjectLiteral(fields) = &self.value else {
            self.push_to_formatter(formatter);

            return;
        };

        formatter.push_declaration_block_start(&render_object_field_name(&self.name));

        for field in fields {
            field.push_to_formatter(formatter);
        }

        formatter.push_declaration_block_end();
    }
}

impl MatchBranch {
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        formatter.push_indent();

        match self {
            Self::Variant {
                case_name,
                field_path,
                span: _,
            } => {
                formatter.output.push_str(case_name);

                for field_name in field_path {
                    formatter.output.push('.');
                    formatter.output.push_str(field_name);
                }
            }
            Self::Fallback { value, span: _ } => {
                formatter.output.push_str("_ ");
                value.push_to_formatter(formatter, ExpressionFormat::Inline);
            }
        }

        formatter.push_newline();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ExpressionFormat {
    Canonical,
    Inline,
}

impl Expression {
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter, expression_format: ExpressionFormat) {
        match self {
            Self::StringLiteral(string_value) => {
                self.push_string_literal_to_formatter(formatter, string_value, expression_format);
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
            Self::Asset(asset) => asset.push_to_formatter(formatter),
            Self::ToolCall(tool_call) => tool_call.push_to_formatter(formatter),
            Self::McpCall(mcp_call) => mcp_call.push_to_formatter(formatter),
            Self::NullFallback(null_fallback) => {
                null_fallback.value.push_to_formatter(formatter, ExpressionFormat::Inline);
                formatter.output.push_str(" ?? ");
                null_fallback.fallback.push_to_formatter(formatter, ExpressionFormat::Inline);
            }
            Self::VariantProjection(variant_projection) => {
                variant_projection.value.push_to_formatter(formatter);
                formatter.output.push('#');
                formatter.output.push_str(&variant_projection.case_name);

                for field_name in &variant_projection.field_path {
                    formatter.output.push('.');
                    formatter.output.push_str(field_name);
                }
            }
            Self::Match(match_expression) => {
                formatter.output.push_str("match ");
                match_expression.value.push_to_formatter(formatter, ExpressionFormat::Inline);
                formatter.output.push_str(" {");
                formatter.push_newline();
                formatter.indentation_depth += 1;

                for match_branch in &match_expression.branches {
                    match_branch.push_to_formatter(formatter);
                }

                formatter.indentation_depth -= 1;
                formatter.push_indent();
                formatter.output.push('}');
            }
            Self::ArrayLiteral(array_items) => {
                self.push_array_literal_to_formatter(formatter, array_items, expression_format);
            }
            Self::ObjectLiteral(object_fields) => {
                self.push_object_literal_to_formatter(formatter, object_fields, expression_format);
            }
        }
    }

    pub(super) fn push_agent_tool_binding_to_formatter(&self, formatter: &mut DslFormatter) {
        match self {
            Self::Reference(reference) => reference.push_to_formatter(formatter),
            Self::ToolCall(tool_call) => tool_call.push_agent_binding_to_formatter(formatter),
            _ => self.push_to_formatter(formatter, ExpressionFormat::Canonical),
        }
    }

    fn push_string_literal_to_formatter(&self, formatter: &mut DslFormatter, string_value: &str, expression_format: ExpressionFormat) {
        if string_value.contains('\n') {
            formatter.push_multiline_string_block(&escape_multiline_string_text(string_value));

            return;
        }

        let quoted_string_literal = render_expression_string_literal(string_value);

        if expression_format == ExpressionFormat::Inline {
            formatter.output.push_str(&quoted_string_literal);

            return;
        }

        if formatter.can_fit_inline_text(&quoted_string_literal) {
            formatter.output.push_str(&quoted_string_literal);

            return;
        }

        let wrapped_multiline_lines = formatter.wrap_multiline_string_value(string_value);

        if wrapped_multiline_lines.len() > 1 {
            formatter.push_multiline_string_block_from_lines(&wrapped_multiline_lines);
        } else {
            formatter.output.push_str(&quoted_string_literal);
        }
    }

    fn push_array_literal_to_formatter(
        &self,
        formatter: &mut DslFormatter,
        array_items: &[Expression],
        expression_format: ExpressionFormat,
    ) {
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

    fn push_object_literal_to_formatter(
        &self,
        formatter: &mut DslFormatter,
        object_fields: &[ObjectField],
        expression_format: ExpressionFormat,
    ) {
        if expression_format == ExpressionFormat::Inline {
            formatter.output.push('{');

            if !object_fields.is_empty() {
                formatter.output.push(' ');
            }

            let mut object_field_iterator = object_fields.iter().peekable();
            while let Some(object_field) = object_field_iterator.next() {
                formatter.output.push_str(&render_object_field_name(&object_field.name));
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

        if let Some(inline_object_literal) = self.inline_object_literal(formatter) {
            if formatter.can_fit_inline_text(&inline_object_literal) {
                formatter.output.push_str(&inline_object_literal);

                return;
            }
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

    fn is_inline_friendly(&self) -> bool {
        match self {
            Self::ArrayLiteral(_) => false,
            Self::ObjectLiteral(object_fields) => {
                object_fields.len() <= 1 && object_fields.iter().all(|object_field| object_field.value.is_inline_friendly())
            }
            Self::StringLiteral(_)
            | Self::StringTemplate(_)
            | Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::Reference(_)
            | Self::FunctionCall(_)
            | Self::Asset(_)
            | Self::ToolCall(_)
            | Self::McpCall(_)
            | Self::NullFallback(_)
            | Self::VariantProjection(_) => true,
            Self::Match(_) => false,
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

    fn inline_object_literal(&self, formatter: &DslFormatter) -> Option<String> {
        let Self::ObjectLiteral(object_fields) = self else {
            return None;
        };

        if object_fields.len() != 1 {
            return None;
        }

        let object_field = &object_fields[0];

        if !object_field.value.is_inline_friendly() {
            return None;
        }

        Some(format!(
            "{{ {}: {} }}",
            object_field.name,
            formatter.inline_expression(&object_field.value)
        ))
    }
}

impl Asset {
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        formatter.output.push_str("asset ");
        self.source.push_to_formatter(formatter, ExpressionFormat::Inline);

        if self.options.is_empty() {
            return;
        }

        formatter.output.push_str(" {");
        formatter.push_newline();
        formatter.indentation_depth += 1;

        for option in &self.options {
            option.push_to_formatter(formatter);
        }

        formatter.indentation_depth -= 1;
        formatter.push_indent();
        formatter.output.push('}');
    }
}

impl StringTemplate {
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        let is_multiline = self.is_multiline();

        if is_multiline {
            formatter.push_multiline_string_block(&self.render_multiline_contents(formatter));
            return;
        }

        let inline_template_contents = self.render_inline_contents(formatter);
        let quoted_inline_template = format!("\"{inline_template_contents}\"");

        if formatter.can_fit_inline_text(&quoted_inline_template) {
            formatter.output.push_str(&quoted_inline_template);
            return;
        }

        let multiline_contents = self.render_multiline_contents(formatter);
        let normalized_multiline_lines = DslFormatter::normalize_multiline_string_lines(&multiline_contents);
        let wrapped_multiline_lines = formatter.wrap_multiline_lines_to_width(&normalized_multiline_lines);

        if wrapped_multiline_lines.len() > 1 {
            formatter.push_multiline_string_block_from_lines(&wrapped_multiline_lines);
        } else {
            formatter.output.push_str(&quoted_inline_template);
        }
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

    fn render_inline_contents(&self, formatter: &DslFormatter) -> String {
        let mut rendered_inline_contents = String::new();

        for string_template_part in &self.parts {
            match string_template_part {
                StringTemplatePart::Text(text) => rendered_inline_contents.push_str(&escape_quoted_string_text(text)),
                StringTemplatePart::Interpolation(expression) => {
                    rendered_inline_contents.push_str("{{ ");
                    rendered_inline_contents.push_str(&formatter.inline_expression(expression));
                    rendered_inline_contents.push_str(" }}");
                }
            }
        }

        rendered_inline_contents
    }
}

impl Reference {
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        formatter.output.push_str(&self.render_path());
    }
}

impl FunctionCall {
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter) {
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

        if self.arguments.len() == 2
            && self.arguments.first().is_some_and(CallArgument::is_inline_friendly)
            && self
                .arguments
                .get(1)
                .is_some_and(CallArgument::is_multiline_object_literal_argument)
        {
            formatter.output.push('(');
            formatter.output.push_str(&self.arguments[0].render_inline(formatter));
            formatter.output.push_str(", ");
            self.arguments[1].push_to_formatter(formatter, ExpressionFormat::Canonical);
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
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter, expression_format: ExpressionFormat) {
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

    fn is_multiline_object_literal_argument(&self) -> bool {
        match self {
            Self::Positional(expression) => expression.is_multiline_object_literal(),
            Self::Named(named_argument) => named_argument.value.is_multiline_object_literal(),
        }
    }
}

impl Expression {
    fn is_multiline_object_literal(&self) -> bool {
        match self {
            Self::ObjectLiteral(object_fields) => object_fields.len() > 1,
            Self::StringLiteral(_)
            | Self::StringTemplate(_)
            | Self::NumberLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::Reference(_)
            | Self::FunctionCall(_)
            | Self::Asset(_)
            | Self::ToolCall(_)
            | Self::McpCall(_)
            | Self::NullFallback(_)
            | Self::VariantProjection(_)
            | Self::Match(_)
            | Self::ArrayLiteral(_) => false,
        }
    }
}
