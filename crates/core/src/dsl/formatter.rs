use std::collections::HashMap;
use std::fmt::Write;

use thiserror::Error;

use super::ast::{
    AgentDeclaration, AgentForLoopPattern, AgentProperty, CallArgument, Declaration, DeclarationKeyword, DynamicBlock, Expression,
    ForClauseKeyword, FunctionCall, ImportKeyword, MatchBranch, McpBatchImportDeclaration, McpCall, McpImportKind, McpImportPropertyName,
    McpPromptBatchImportDeclaration, McpPromptImportDeclaration, McpResourceBatchImportDeclaration, McpResourceImportDeclaration,
    McpToolBatchImportDeclaration, ModelUsage, ObjectField, Reference, StringTemplate, StringTemplatePart, ToolCall, ToolCallPropertyName,
    ToolDeclaration, ToolPropertyName, TypeExpression, TypedField, Workflow,
};
use super::parse_workflow;
use super::parser::DslParseError;
use super::structure::{self, DslProperty};

const MAX_LINE_WIDTH: usize = 120;
const WRAP_WIDTH_BUFFER: usize = 12;

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

    fn push_agent_property_type_block(&mut self, property_name: &str, fields: &[TypedField]) {
        self.push_indent();
        self.output.push_str(property_name);
        self.output.push_str(" {");
        self.push_newline();
        self.indentation_depth += 1;

        for field in fields {
            field.push_to_formatter(self);
        }

        self.indentation_depth -= 1;
        self.push_indent();
        self.output.push('}');
        self.push_newline();
    }

    fn push_multiline_string_block(&mut self, escaped_multiline_contents: &str) {
        let normalized_multiline_lines = Self::normalize_multiline_string_lines(escaped_multiline_contents);
        let wrapped_multiline_lines = self.wrap_multiline_lines_to_width(&normalized_multiline_lines);

        self.output.push_str("\"\"\"");
        self.push_newline();
        self.indentation_depth += 1;

        for multiline_content_line in wrapped_multiline_lines {
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

    fn wrap_multiline_lines_to_width(&self, multiline_content_lines: &[String]) -> Vec<String> {
        let line_width_limit = self.multiline_content_width_limit();
        let mut wrapped_multiline_lines = Vec::new();

        for multiline_content_line in multiline_content_lines {
            if multiline_content_line.trim().is_empty() {
                wrapped_multiline_lines.push(String::new());
                continue;
            }

            wrapped_multiline_lines.extend(wrap_text_line_by_words(multiline_content_line, line_width_limit));
        }

        wrapped_multiline_lines
    }

    fn can_fit_inline_text(&self, inline_text: &str) -> bool {
        !inline_text.contains('\n') && self.current_line_width() + inline_text.chars().count() <= MAX_LINE_WIDTH
    }

    fn multiline_content_width_limit(&self) -> usize {
        MAX_LINE_WIDTH.saturating_sub((self.indentation_depth + 1) * 4).max(20)
    }

    fn current_line_width(&self) -> usize {
        self.output.rsplit('\n').next().map_or(0, |line_text| line_text.chars().count())
    }

    fn wrap_multiline_string_value(&self, raw_string: &str) -> Vec<String> {
        wrap_text_line_by_words(raw_string.trim(), self.multiline_content_width_limit())
            .into_iter()
            .map(|wrapped_line| escape_multiline_string_text(&wrapped_line))
            .collect::<Vec<_>>()
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
                formatter.push_declaration_block_start(&format!(
                    "{} {} from {}",
                    DeclarationKeyword::Provider.as_str(),
                    provider_declaration.name,
                    provider_declaration.driver_name
                ));

                for object_field in &provider_declaration.properties {
                    object_field.push_config_property_to_formatter(formatter);
                }

                formatter.push_declaration_block_end();
            }
            Self::Model(model_declaration) => {
                formatter.push_declaration_block_start(&format!(
                    "{} {} from {}",
                    DeclarationKeyword::Model.as_str(),
                    model_declaration.name,
                    model_declaration.provider_name
                ));

                for object_field in &model_declaration.properties {
                    object_field.push_config_property_to_formatter(formatter);
                }

                formatter.push_declaration_block_end();
            }
            Self::McpServer(mcp_server_declaration) => {
                formatter.push_declaration_block_start(&format!("{} {}", DeclarationKeyword::Mcp.as_str(), mcp_server_declaration.name));

                for object_field in &mcp_server_declaration.properties {
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
            Self::Tool(tool_declaration) => tool_declaration.push_to_formatter(formatter),
            Self::McpBatch(batch_import_declaration) => batch_import_declaration.push_to_formatter(formatter),
            Self::McpToolBatch(tool_batch_import_declaration) => tool_batch_import_declaration.push_to_formatter(formatter),
            Self::McpResourceBatch(resource_batch_import_declaration) => resource_batch_import_declaration.push_to_formatter(formatter),
            Self::McpPromptBatch(prompt_batch_import_declaration) => prompt_batch_import_declaration.push_to_formatter(formatter),
            Self::McpResource(resource_import_declaration) => resource_import_declaration.push_to_formatter(formatter),
            Self::McpPrompt(prompt_import_declaration) => prompt_import_declaration.push_to_formatter(formatter),
            Self::Dynamic(dynamic_block) => dynamic_block.push_to_formatter(formatter),
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
            declaration_header.push_str(&loop_declaration.pattern.render_for_clause());
            declaration_header.push(' ');
            declaration_header.push_str(ForClauseKeyword::In.as_str());
            declaration_header.push(' ');
            declaration_header.push_str(&formatter.inline_expression(&loop_declaration.iterable));
        }

        formatter.push_declaration_block_start(&declaration_header);

        let rendered_properties = self
            .properties
            .iter()
            .map(|agent_property| agent_property.render_for_agent_block(formatter.indentation_depth))
            .collect::<Vec<_>>();

        for (property_index, rendered_property) in rendered_properties.iter().enumerate() {
            if property_index > 0 {
                let previous_property_is_multiline = rendered_properties[property_index.saturating_sub(1)].is_multiline;
                let current_property_is_multiline = rendered_property.is_multiline;

                if previous_property_is_multiline || (current_property_is_multiline && !rendered_property.is_output_property) {
                    formatter.push_newline();
                }
            }

            formatter.output.push_str(&rendered_property.text);
        }

        formatter.push_declaration_block_end();
    }
}

impl McpToolBatchImportDeclaration {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        let header = format!(
            "{} mcp.{}.{}",
            ImportKeyword::From.as_str(),
            self.server_name,
            McpImportKind::Tool.as_str()
        );

        formatter.push_declaration_block_start(&header);

        if !self.input_fields.is_empty() {
            formatter.push_declaration_block_start(ToolPropertyName::Input.as_str());

            for typed_field in &self.input_fields {
                typed_field.push_to_formatter(formatter);
            }

            formatter.push_declaration_block_end();

            if !self.fixed_binding_fields.is_empty() || self.max_calls.is_some() || !self.output_fields.is_empty() || !self.tools.is_empty()
            {
                formatter.push_newline();
            }
        }

        if !self.fixed_binding_fields.is_empty() {
            formatter.push_declaration_block_start(ToolPropertyName::Bindings.as_str());

            for object_field in &self.fixed_binding_fields {
                object_field.push_to_formatter(formatter);
            }

            formatter.push_declaration_block_end();

            if self.max_calls.is_some() || !self.output_fields.is_empty() || !self.tools.is_empty() {
                formatter.push_newline();
            }
        }

        if let Some(max_calls) = self.max_calls {
            formatter.push_line(&format!(
                "{}: {max_calls}",
                super::ast::McpToolBatchImportPropertyName::MaxCalls.definition().name
            ));

            if !self.output_fields.is_empty() || !self.items.is_empty() {
                formatter.push_newline();
            }
        }

        if !self.output_fields.is_empty() {
            formatter.push_declaration_block_start(ToolPropertyName::Output.as_str());

            for typed_field in &self.output_fields {
                typed_field.push_to_formatter(formatter);
            }

            formatter.push_declaration_block_end();

            if !self.items.is_empty() {
                formatter.push_newline();
            }
        }

        for item in &self.items {
            item.push_to_formatter(formatter);
        }

        formatter.push_declaration_block_end();
    }
}

impl McpBatchImportDeclaration {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        let header = format!("{} mcp.{}", ImportKeyword::From.as_str(), self.server_name);

        formatter.push_declaration_block_start(&header);

        if !self.input_fields.is_empty() {
            formatter.push_declaration_block_start(ToolPropertyName::Input.as_str());

            for typed_field in &self.input_fields {
                typed_field.push_to_formatter(formatter);
            }

            formatter.push_declaration_block_end();

            if !self.fixed_binding_fields.is_empty()
                || self.max_calls.is_some()
                || !self.output_fields.is_empty()
                || !self.resource_items.is_empty()
                || !self.prompt_items.is_empty()
                || !self.tool_items.is_empty()
            {
                formatter.push_newline();
            }
        }

        if !self.fixed_binding_fields.is_empty() {
            formatter.push_declaration_block_start(super::ast::McpToolBatchImportPropertyName::Bindings.definition().name);

            for object_field in &self.fixed_binding_fields {
                object_field.push_to_formatter(formatter);
            }

            formatter.push_declaration_block_end();

            if self.max_calls.is_some()
                || !self.output_fields.is_empty()
                || !self.resource_items.is_empty()
                || !self.prompt_items.is_empty()
                || !self.tool_items.is_empty()
            {
                formatter.push_newline();
            }
        }

        if let Some(max_calls) = self.max_calls {
            formatter.push_line(&format!("{}: {max_calls}", ToolPropertyName::MaxCalls.as_str()));

            if !self.output_fields.is_empty()
                || !self.resource_items.is_empty()
                || !self.prompt_items.is_empty()
                || !self.tool_items.is_empty()
            {
                formatter.push_newline();
            }
        }

        if !self.output_fields.is_empty() {
            formatter.push_declaration_block_start(ToolPropertyName::Output.as_str());

            for typed_field in &self.output_fields {
                typed_field.push_to_formatter(formatter);
            }

            formatter.push_declaration_block_end();

            if !self.resource_items.is_empty() || !self.prompt_items.is_empty() || !self.tool_items.is_empty() {
                formatter.push_newline();
            }
        }

        for item in &self.resource_items {
            item.push_to_formatter(formatter);
        }

        for item in &self.prompt_items {
            item.push_to_formatter(formatter);
        }

        for item in &self.tool_items {
            item.push_to_formatter(formatter);
        }

        formatter.push_declaration_block_end();
    }
}

impl McpResourceBatchImportDeclaration {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        let header = format!(
            "{} mcp.{}.{}",
            ImportKeyword::From.as_str(),
            self.server_name,
            McpImportKind::Resource.as_str()
        );

        formatter.push_declaration_block_start(&header);

        if !self.parameters.is_empty() {
            formatter.push_declaration_block_start(McpImportPropertyName::Bindings.as_str());

            for parameter in &self.parameters {
                parameter.push_to_formatter(formatter);
            }

            formatter.push_declaration_block_end();

            if !self.items.is_empty() {
                formatter.push_newline();
            }
        }

        for item in &self.items {
            item.push_to_formatter(formatter);
        }

        formatter.push_declaration_block_end();
    }
}

impl McpPromptBatchImportDeclaration {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        let header = format!(
            "{} mcp.{}.{}",
            ImportKeyword::From.as_str(),
            self.server_name,
            McpImportKind::Prompt.as_str()
        );

        formatter.push_declaration_block_start(&header);

        if !self.parameters.is_empty() {
            formatter.push_declaration_block_start(McpImportPropertyName::Bindings.as_str());

            for parameter in &self.parameters {
                parameter.push_to_formatter(formatter);
            }

            formatter.push_declaration_block_end();

            if !self.items.is_empty() {
                formatter.push_newline();
            }
        }

        for item in &self.items {
            item.push_to_formatter(formatter);
        }

        formatter.push_declaration_block_end();
    }
}

impl super::ast::McpResourceBatchImportItem {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        let mut header = format!("{} {}", DeclarationKeyword::Resource.as_str(), self.source_name);

        if let Some(alias) = &self.alias {
            let _ = write!(header, " {} {alias}", ImportKeyword::As.as_str());
        }

        if self.parameters.is_empty() {
            formatter.push_line(&header);

            return;
        }

        formatter.push_declaration_block_start(&header);
        formatter.push_declaration_block_start(McpImportPropertyName::Bindings.as_str());

        for parameter in &self.parameters {
            parameter.push_to_formatter(formatter);
        }

        formatter.push_declaration_block_end();
        formatter.push_declaration_block_end();
    }
}

impl super::ast::McpPromptBatchImportItem {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        let mut header = format!("{} {}", DeclarationKeyword::Prompt.as_str(), self.source_name);

        if let Some(alias) = &self.alias {
            let _ = write!(header, " {} {alias}", ImportKeyword::As.as_str());
        }

        if self.parameters.is_empty() {
            formatter.push_line(&header);

            return;
        }

        formatter.push_declaration_block_start(&header);
        formatter.push_declaration_block_start(McpImportPropertyName::Bindings.as_str());

        for parameter in &self.parameters {
            parameter.push_to_formatter(formatter);
        }

        formatter.push_declaration_block_end();
        formatter.push_declaration_block_end();
    }
}

impl super::ast::McpToolBatchImportItem {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        let wire_tool_name = self.source_name.replace('-', "_");
        let header = if let Some(alias) = &self.alias {
            format!(
                "{} {} {} {}",
                DeclarationKeyword::Tool.as_str(),
                wire_tool_name,
                ImportKeyword::As.as_str(),
                alias
            )
        } else {
            format!("{} {}", DeclarationKeyword::Tool.as_str(), wire_tool_name)
        };

        if self.input_fields.is_empty() && self.fixed_binding_fields.is_empty() && self.max_calls.is_none() && self.output_fields.is_empty()
        {
            formatter.push_line(&header);

            return;
        }

        formatter.push_declaration_block_start(&header);

        if let Some(max_calls) = self.max_calls {
            formatter.push_line(&format!("{}: {max_calls}", ToolPropertyName::MaxCalls.as_str()));

            if !self.input_fields.is_empty() || !self.fixed_binding_fields.is_empty() || !self.output_fields.is_empty() {
                formatter.push_newline();
            }
        }

        if !self.input_fields.is_empty() {
            formatter.push_declaration_block_start(ToolPropertyName::Input.as_str());

            for typed_field in &self.input_fields {
                typed_field.push_to_formatter(formatter);
            }

            formatter.push_declaration_block_end();

            if !self.fixed_binding_fields.is_empty() || !self.output_fields.is_empty() {
                formatter.push_newline();
            }
        }

        if !self.fixed_binding_fields.is_empty() {
            formatter.push_declaration_block_start(ToolPropertyName::Bindings.as_str());

            for object_field in &self.fixed_binding_fields {
                object_field.push_to_formatter(formatter);
            }

            formatter.push_declaration_block_end();

            if !self.output_fields.is_empty() {
                formatter.push_newline();
            }
        }

        if !self.output_fields.is_empty() {
            formatter.push_declaration_block_start(ToolPropertyName::Output.as_str());

            for typed_field in &self.output_fields {
                typed_field.push_to_formatter(formatter);
            }

            formatter.push_declaration_block_end();
        }

        formatter.push_declaration_block_end();
    }
}

impl ToolDeclaration {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        if self.imported {
            self.push_import_to_formatter(formatter);

            return;
        }

        formatter.push_declaration_block_start(&format!("{} {}", DeclarationKeyword::Tool.as_str(), self.name));

        if let Some(description) = &self.description {
            formatter.push_line(&format!(
                "{}: {}",
                ToolPropertyName::Description.as_str(),
                render_plain_string_literal(description)
            ));

            if !self.input_fields.is_empty()
                || !self.binding_fields.is_empty()
                || !self.fixed_binding_fields.is_empty()
                || self.max_calls.is_some()
                || !self.output_fields.is_empty()
            {
                formatter.push_newline();
            }
        }

        if let Some(max_calls) = self.max_calls {
            formatter.push_line(&format!("{}: {max_calls}", ToolPropertyName::MaxCalls.as_str()));

            if !self.input_fields.is_empty()
                || !self.binding_fields.is_empty()
                || !self.fixed_binding_fields.is_empty()
                || !self.output_fields.is_empty()
            {
                formatter.push_newline();
            }
        }

        if !self.input_fields.is_empty() {
            formatter.push_declaration_block_start(ToolPropertyName::Input.as_str());

            for typed_field in &self.input_fields {
                typed_field.push_to_formatter(formatter);
            }

            formatter.push_declaration_block_end();

            if !self.binding_fields.is_empty() || !self.fixed_binding_fields.is_empty() || !self.output_fields.is_empty() {
                formatter.push_newline();
            }
        }

        if !self.binding_fields.is_empty() || !self.fixed_binding_fields.is_empty() {
            formatter.push_declaration_block_start(ToolPropertyName::Bindings.as_str());

            for typed_field in &self.binding_fields {
                typed_field.push_to_formatter(formatter);
            }

            for object_field in &self.fixed_binding_fields {
                object_field.push_to_formatter(formatter);
            }

            formatter.push_declaration_block_end();

            if !self.output_fields.is_empty() {
                formatter.push_newline();
            }
        }

        if !self.output_fields.is_empty() {
            formatter.push_declaration_block_start(ToolPropertyName::Output.as_str());

            for typed_field in &self.output_fields {
                typed_field.push_to_formatter(formatter);
            }

            formatter.push_declaration_block_end();
        }

        formatter.push_declaration_block_end();
    }

    fn push_import_to_formatter(&self, formatter: &mut DslFormatter) {
        let Some(super::ast::ToolSource::Mcp(mcp_tool_source)) = &self.source else {
            formatter.push_declaration_block_start(&format!("{} {}", DeclarationKeyword::Tool.as_str(), self.name));
            formatter.push_declaration_block_end();

            return;
        };
        let wire_tool_name = mcp_tool_source.tool_name.replace('-', "_");
        let source_path = if let Some(server_name) = &mcp_tool_source.server_name {
            format!("mcp.{server_name}.tool.{wire_tool_name}")
        } else {
            format!("mcp.tool.{wire_tool_name}")
        };
        let header = format!(
            "{} {} {} {source_path}",
            DeclarationKeyword::Tool.as_str(),
            self.name,
            ImportKeyword::From.as_str()
        );

        if self.input_fields.is_empty() && self.fixed_binding_fields.is_empty() && self.output_fields.is_empty() && self.max_calls.is_none()
        {
            formatter.push_line(&header);

            return;
        }

        formatter.push_declaration_block_start(&header);

        if let Some(max_calls) = self.max_calls {
            formatter.push_line(&format!("{}: {max_calls}", ToolPropertyName::MaxCalls.as_str()));

            if !self.input_fields.is_empty() || !self.fixed_binding_fields.is_empty() || !self.output_fields.is_empty() {
                formatter.push_newline();
            }
        }

        if !self.input_fields.is_empty() {
            formatter.push_declaration_block_start(ToolPropertyName::Input.as_str());

            for input_field in &self.input_fields {
                input_field.push_to_formatter(formatter);
            }

            formatter.push_declaration_block_end();

            if !self.fixed_binding_fields.is_empty() || !self.output_fields.is_empty() {
                formatter.push_newline();
            }
        }

        if !self.fixed_binding_fields.is_empty() {
            formatter.push_declaration_block_start(ToolPropertyName::Bindings.as_str());

            for fixed_binding_field in &self.fixed_binding_fields {
                fixed_binding_field.push_to_formatter(formatter);
            }

            formatter.push_declaration_block_end();

            if !self.output_fields.is_empty() {
                formatter.push_newline();
            }
        }

        if !self.output_fields.is_empty() {
            formatter.push_declaration_block_start(ToolPropertyName::Output.as_str());

            for output_field in &self.output_fields {
                output_field.push_to_formatter(formatter);
            }

            formatter.push_declaration_block_end();
        }

        formatter.push_declaration_block_end();
    }
}

impl McpResourceImportDeclaration {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        let header = format!(
            "{} {} {} {}",
            DeclarationKeyword::Resource.as_str(),
            self.name,
            ImportKeyword::From.as_str(),
            self.source.render_path()
        );

        push_mcp_import_with_parameters(formatter, &header, &self.parameters);
    }
}

impl McpPromptImportDeclaration {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        let header = format!(
            "{} {} {} {}",
            DeclarationKeyword::Prompt.as_str(),
            self.name,
            ImportKeyword::From.as_str(),
            self.source.render_path()
        );

        push_mcp_import_with_parameters(formatter, &header, &self.parameters);
    }
}

fn push_mcp_import_with_parameters(formatter: &mut DslFormatter, header: &str, parameters: &[ObjectField]) {
    if parameters.is_empty() {
        formatter.push_line(header);

        return;
    }

    formatter.push_declaration_block_start(header);
    formatter.push_declaration_block_start(McpImportPropertyName::Bindings.as_str());

    for parameter in parameters {
        parameter.push_to_formatter(formatter);
    }

    formatter.push_declaration_block_end();
    formatter.push_declaration_block_end();
}

impl AgentForLoopPattern {
    fn render_for_clause(&self) -> String {
        match self {
            Self::Identifier(identifier) => identifier.clone(),
            Self::ObjectDestructuring(field_names) => format!("{{ {} }}", field_names.join(", ")),
        }
    }
}

impl AgentProperty {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        match self {
            Self::Dynamic(dynamic_block) => dynamic_block.push_to_formatter(formatter),
            Self::Model(model_usage) => model_usage.push_to_formatter(formatter),
            Self::InvalidModel(expression) => {
                formatter.push_agent_property_expression(structure::Agent::new().model.definition().name, expression);
            }
            Self::Instruction(expression) => {
                formatter.push_agent_property_expression(structure::Agent::new().instruction.definition().name, expression);
            }
            Self::Output { fields, span: _ } => {
                let agent = structure::Agent::new();

                formatter.push_agent_property_type_block(
                    agent.output.expect("agent structure should include output").definition().name,
                    fields,
                );
            }
            Self::Context(expression) => {
                let agent = structure::Agent::new();

                formatter.push_agent_property_expression(
                    agent.context.expect("agent structure should include context").definition().name,
                    expression,
                );
            }
            Self::Uses(expression) => {
                self.push_agent_binding_list_property(formatter, structure::Agent::new().uses[0].definition().name, expression)
            }
            Self::Unknown { name: _, span: _ } => {}
        }
    }

    fn push_agent_binding_list_property(&self, formatter: &mut DslFormatter, property_name: &str, expression: &Expression) {
        let Expression::ArrayLiteral(tool_bindings) = expression else {
            formatter.push_agent_property_expression(property_name, expression);

            return;
        };

        formatter.push_indent();
        formatter.output.push_str(property_name);
        formatter.output.push_str(": ");

        if tool_bindings.is_empty() {
            formatter.output.push_str("[]");
            formatter.push_newline();

            return;
        }

        if let Some(inline_tool_bindings) = self.inline_agent_tool_bindings(formatter, tool_bindings) {
            if formatter.can_fit_inline_text(&inline_tool_bindings) {
                formatter.output.push_str(&inline_tool_bindings);
                formatter.push_newline();

                return;
            }
        }

        formatter.output.push('[');
        formatter.push_newline();
        formatter.indentation_depth += 1;

        for tool_binding in tool_bindings {
            formatter.push_indent();
            tool_binding.push_agent_tool_binding_to_formatter(formatter);
            formatter.output.push(',');
            formatter.push_newline();
        }

        formatter.indentation_depth -= 1;
        formatter.push_indent();
        formatter.output.push(']');
        formatter.push_newline();
    }

    fn inline_agent_tool_bindings(&self, formatter: &DslFormatter, tool_bindings: &[Expression]) -> Option<String> {
        let mut inline_tool_bindings = Vec::new();

        for tool_binding in tool_bindings {
            match tool_binding {
                Expression::Reference(reference) => {
                    let mut reference_formatter = DslFormatter::new();
                    reference.push_to_formatter(&mut reference_formatter);
                    inline_tool_bindings.push(reference_formatter.output);
                }
                Expression::ToolCall(tool_call) if tool_call.binding_fields.is_empty() => {
                    let mut tool_call_formatter = DslFormatter::new();
                    tool_call.push_agent_binding_to_formatter(&mut tool_call_formatter);
                    inline_tool_bindings.push(tool_call_formatter.output);
                }
                _ => return None,
            }
        }

        let inline_expression = format!("[{}]", inline_tool_bindings.join(", "));

        if formatter.can_fit_inline_text(&inline_expression) {
            Some(inline_expression)
        } else {
            None
        }
    }

    fn render_for_agent_block(&self, indentation_depth: usize) -> RenderedAgentProperty {
        let mut property_formatter = DslFormatter {
            output: String::new(),
            indentation_depth,
        };

        self.push_to_formatter(&mut property_formatter);

        let property_body_without_trailing_newline = property_formatter
            .output
            .strip_suffix('\n')
            .unwrap_or(property_formatter.output.as_str());
        let property_is_multiline = property_body_without_trailing_newline.contains('\n');

        RenderedAgentProperty {
            text: property_formatter.output,
            is_multiline: property_is_multiline,
            is_output_property: matches!(self, Self::Output { fields: _, span: _ }),
        }
    }
}

impl DynamicBlock {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        formatter.push_indent();
        formatter.output.push_str(structure::Agent::new().dynamic[0].definition().name);
        formatter.output.push(' ');
        formatter.output.push('{');
        formatter.push_newline();
        formatter.indentation_depth += 1;

        for field in &self.fields {
            field.push_to_formatter(formatter);
        }

        formatter.indentation_depth -= 1;
        formatter.push_indent();
        formatter.output.push('}');
        formatter.push_newline();
    }
}

impl ModelUsage {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        formatter.push_indent();
        formatter.output.push_str(structure::Agent::new().model.definition().name);
        formatter.output.push_str(": ");
        self.reference.push_to_formatter(formatter);

        if self.properties.is_empty() {
            formatter.push_newline();

            return;
        }

        formatter.output.push_str(" {");
        formatter.push_newline();
        formatter.indentation_depth += 1;

        for property in &self.properties {
            property.push_config_property_to_formatter(formatter);
        }

        formatter.indentation_depth -= 1;
        formatter.push_indent();
        formatter.output.push('}');
        formatter.push_newline();
    }
}

struct RenderedAgentProperty {
    text: String,
    is_multiline: bool,
    is_output_property: bool,
}

impl TypedField {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        if let Some(description) = &self.description {
            for description_line in description.lines() {
                formatter.push_indent();
                formatter.output.push_str("///");

                if !description_line.is_empty() {
                    formatter.output.push(' ');
                    formatter.output.push_str(description_line);
                }

                formatter.push_newline();
            }
        }

        formatter.push_indent();
        formatter.output.push_str(&self.name);
        formatter.output.push_str(": ");
        self.field_type.push_to_formatter(formatter);

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
            Self::AnyObject => formatter.output.push_str("object"),
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
            Self::Variant { discriminator, cases } => {
                formatter.output.push_str("variant ");
                formatter.output.push_str(discriminator);
                formatter.output.push_str(" {");
                formatter.push_newline();
                formatter.indentation_depth += 1;

                for variant_case in cases {
                    formatter.push_indent();
                    formatter.output.push_str(&variant_case.name);
                    formatter.output.push_str(" {");
                    formatter.push_newline();
                    formatter.indentation_depth += 1;

                    for typed_field in &variant_case.fields {
                        typed_field.push_to_formatter(formatter);
                    }

                    formatter.indentation_depth -= 1;
                    formatter.push_indent();
                    formatter.output.push('}');
                    formatter.push_newline();
                }

                formatter.indentation_depth -= 1;
                formatter.push_indent();
                formatter.output.push('}');
            }
            Self::Union(union_members) => {
                if Self::push_nullable_union_to_formatter(union_members, formatter) {
                    return;
                }

                if Self::push_string_enum_union_to_formatter(union_members, formatter) {
                    return;
                }

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

    fn push_nullable_union_to_formatter(union_members: &[Self], formatter: &mut DslFormatter) -> bool {
        if !union_members.iter().any(|union_member| matches!(union_member, Self::Null)) {
            return false;
        }

        let non_null_members = union_members
            .iter()
            .filter(|union_member| !matches!(union_member, Self::Null))
            .collect::<Vec<_>>();

        if non_null_members.len() == 1 {
            formatter.output.push_str("maybe ");
            non_null_members[0].push_to_formatter(formatter);

            return true;
        }

        if non_null_members
            .iter()
            .all(|union_member| matches!(union_member, Self::StringEnum(_)))
        {
            formatter.output.push_str("maybe ");
            Self::push_string_enum_members_to_formatter(non_null_members.as_slice(), formatter);

            return true;
        }

        false
    }

    fn push_string_enum_union_to_formatter(union_members: &[Self], formatter: &mut DslFormatter) -> bool {
        if !union_members.iter().all(|union_member| matches!(union_member, Self::StringEnum(_))) {
            return false;
        }

        let enum_members = union_members.iter().collect::<Vec<_>>();
        Self::push_string_enum_members_to_formatter(enum_members.as_slice(), formatter);

        true
    }

    fn push_string_enum_members_to_formatter(enum_members: &[&Self], formatter: &mut DslFormatter) {
        formatter.output.push_str("enum { ");

        let mut enum_member_iterator = enum_members.iter().peekable();

        while let Some(enum_member) = enum_member_iterator.next() {
            if let Self::StringEnum(enum_value) = enum_member {
                formatter.output.push_str(enum_value);
            }

            if enum_member_iterator.peek().is_some() {
                formatter.output.push_str(", ");
            }
        }

        formatter.output.push_str(" }");
    }
}

impl ObjectField {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        formatter.push_indent();
        formatter.output.push_str(&render_object_field_name(&self.name));
        formatter.output.push_str(": ");
        self.value.push_to_formatter(formatter, ExpressionFormat::Canonical);
        formatter.push_newline();
    }

    fn push_config_property_to_formatter(&self, formatter: &mut DslFormatter) {
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
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
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
enum ExpressionFormat {
    Canonical,
    Inline,
}

impl Expression {
    fn push_to_formatter(&self, formatter: &mut DslFormatter, expression_format: ExpressionFormat) {
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

    fn push_agent_tool_binding_to_formatter(&self, formatter: &mut DslFormatter) {
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

impl StringTemplate {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
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

impl ToolCall {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        formatter.output.push_str("call ");
        self.callee.push_to_formatter(formatter);
        formatter.output.push_str(" {");
        formatter.push_newline();
        formatter.indentation_depth += 1;

        if !self.input_fields.is_empty() {
            formatter.push_declaration_block_start(ToolCallPropertyName::Input.definition().name);

            for object_field in &self.input_fields {
                object_field.push_to_formatter(formatter);
            }

            formatter.push_declaration_block_end();

            if !self.binding_fields.is_empty() || self.max_calls.is_some() {
                formatter.push_newline();
            }
        }

        if !self.binding_fields.is_empty() {
            formatter.push_declaration_block_start(ToolCallPropertyName::Bindings.definition().name);

            for object_field in &self.binding_fields {
                object_field.push_to_formatter(formatter);
            }

            formatter.push_declaration_block_end();

            if self.max_calls.is_some() {
                formatter.push_newline();
            }
        }

        if let Some(max_calls) = self.max_calls {
            formatter.push_line(&format!("{}: {max_calls}", ToolCallPropertyName::MaxCalls.definition().name));
        }

        formatter.indentation_depth -= 1;
        formatter.push_indent();
        formatter.output.push('}');
    }

    fn push_agent_binding_to_formatter(&self, formatter: &mut DslFormatter) {
        self.callee.push_to_formatter(formatter);

        if self.binding_fields.is_empty() && self.max_calls.is_none() {
            return;
        }

        formatter.output.push_str(" {");
        formatter.push_newline();
        formatter.indentation_depth += 1;
        if !self.binding_fields.is_empty() {
            formatter.push_declaration_block_start(ToolCallPropertyName::Bindings.definition().name);

            for object_field in &self.binding_fields {
                object_field.push_to_formatter(formatter);
            }

            formatter.push_declaration_block_end();

            if self.max_calls.is_some() {
                formatter.push_newline();
            }
        }

        if let Some(max_calls) = self.max_calls {
            formatter.push_line(&format!("{}: {max_calls}", ToolCallPropertyName::MaxCalls.definition().name));
        }

        formatter.indentation_depth -= 1;
        formatter.push_indent();
        formatter.output.push('}');
    }
}

impl McpCall {
    fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        formatter.output.push_str(self.operation.as_str());
        formatter.output.push(' ');
        self.callee.push_to_formatter(formatter);

        if self.parameter_fields.is_empty() {
            return;
        }

        formatter.output.push_str(" {");
        formatter.push_newline();
        formatter.indentation_depth += 1;
        formatter.push_declaration_block_start(McpImportPropertyName::Bindings.as_str());

        for parameter_field in &self.parameter_fields {
            parameter_field.push_to_formatter(formatter);
        }

        formatter.push_declaration_block_end();
        formatter.indentation_depth -= 1;
        formatter.push_indent();
        formatter.output.push('}');
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
            | Self::ToolCall(_)
            | Self::McpCall(_)
            | Self::NullFallback(_)
            | Self::VariantProjection(_)
            | Self::Match(_)
            | Self::ArrayLiteral(_) => false,
        }
    }
}

fn find_wrap_split_index(text: &str, width_limit: usize) -> Option<usize> {
    let mut byte_index = 0_usize;
    let mut character_index = 0_usize;
    let mut last_whitespace_character_index = None;
    let mut is_inside_interpolation = false;

    while byte_index < text.len() {
        let remaining_text = &text[byte_index..];

        if !is_inside_interpolation && remaining_text.starts_with("{{") {
            is_inside_interpolation = true;
            byte_index += 2;
            character_index += 2;
            continue;
        }

        if is_inside_interpolation && remaining_text.starts_with("}}") {
            is_inside_interpolation = false;
            byte_index += 2;
            character_index += 2;
            continue;
        }

        if character_index >= width_limit {
            break;
        }

        let Some(current_character) = remaining_text.chars().next() else {
            break;
        };

        if current_character.is_whitespace() && !is_inside_interpolation {
            last_whitespace_character_index = Some(character_index);
        }

        byte_index += current_character.len_utf8();
        character_index += 1;
    }

    last_whitespace_character_index
}

fn wrap_text_line_by_words(text_line: &str, width_limit: usize) -> Vec<String> {
    let trimmed_text_line = text_line.trim();

    if trimmed_text_line.is_empty() {
        return vec![String::new()];
    }

    let mut wrapped_lines = Vec::new();
    let mut remaining_text = trimmed_text_line.to_owned();
    let width_limit_with_buffer = width_limit.saturating_add(WRAP_WIDTH_BUFFER);

    while remaining_text.chars().count() > width_limit_with_buffer {
        let split_character_index =
            find_wrap_split_index(&remaining_text, width_limit).or_else(|| find_wrap_split_index(&remaining_text, width_limit_with_buffer));
        let Some(split_character_index) = split_character_index else {
            break;
        };

        if split_character_index == 0 {
            break;
        }

        let wrapped_line = remaining_text
            .chars()
            .take(split_character_index)
            .collect::<String>()
            .trim_end()
            .to_owned();

        if wrapped_line.is_empty() {
            break;
        }

        wrapped_lines.push(wrapped_line);

        let wrapped_remainder = remaining_text
            .chars()
            .skip(split_character_index)
            .collect::<String>()
            .trim_start()
            .to_owned();

        wrapped_remainder.clone_into(&mut remaining_text);
    }

    wrapped_lines.push(remaining_text.trim_end().to_owned());
    wrapped_lines
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

fn render_object_field_name(field_name: &str) -> String {
    if is_identifier_name(field_name) {
        return field_name.to_string();
    }

    render_plain_string_literal(field_name)
}

fn is_identifier_name(value: &str) -> bool {
    let mut characters = value.chars();
    let Some(first_character) = characters.next() else {
        return false;
    };

    if !first_character.is_ascii_alphabetic() && first_character != '_' {
        return false;
    }

    characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
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

                if comment_text.trim_start().starts_with("///") {
                    source_line_analyses.push(SourceLineAnalysis {
                        line_number: line_index + 1,
                        code_text,
                        comment: None,
                        is_within_multiline_string: starts_inside_multiline_string,
                    });

                    continue;
                }

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
    let compact_signature = line_text.chars().filter(|character| !character.is_whitespace()).collect::<String>();
    let normalized_signature = compact_signature.trim_end_matches(',').to_owned();

    if normalized_signature.is_empty() {
        return None;
    }

    Some(normalized_signature)
}

fn map_source_lines_to_formatted_lines(
    source_code_signature_lines: &[SourceCodeSignatureLine],
    formatted_code_signature_lines: &[FormattedCodeSignatureLine],
) -> HashMap<usize, usize> {
    let mut source_to_formatted_map = HashMap::new();
    let mut formatted_cursor = 0_usize;

    for source_code_signature_line in source_code_signature_lines {
        let relative_match_index = formatted_code_signature_lines[formatted_cursor..]
            .iter()
            .position(|formatted_code_signature_line| formatted_code_signature_line.signature == source_code_signature_line.signature);

        let Some(relative_match_index) = relative_match_index else {
            continue;
        };

        let absolute_match_index = formatted_cursor + relative_match_index;
        let formatted_code_signature_line = &formatted_code_signature_lines[absolute_match_index];

        source_to_formatted_map.insert(
            source_code_signature_line.source_line_number,
            formatted_code_signature_line.formatted_line_index,
        );

        formatted_cursor = absolute_match_index + 1;
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
        let source_text =
            "provider openai from openai{}\nmodel openai_model from openai{id:\"gpt-4o-mini\"}\n\noutput { result: \"ok\" }\n";

        let expected_output =
            "provider openai from openai {\n}\n\nmodel openai_model from openai {\n    id: \"gpt-4o-mini\"\n}\n\noutput {\n    result: \"ok\"\n}\n";

        let formatted_source = format_workflow_source(source_text).expect("representative workflow should format successfully");

        assert_eq!(formatted_source, expected_output);
    }

    #[test]
    fn formatter_places_standalone_comment_before_next_declaration_when_source_is_single_line_block() {
        let source_text =
            "// provider declaration\nprovider openai from openai {\n// provider driver\n}\n\n// output heading\noutput { value: \"ok\" }\n";

        let expected_output =
            "// provider declaration\nprovider openai from openai {\n// provider driver\n}\n\n// output heading\noutput {\n    value: \"ok\"\n}\n";

        let formatted_source = format_workflow_source(source_text).expect("workflow with standalone comment should format successfully");

        assert_eq!(formatted_source, expected_output);
    }

    #[test]
    fn formatter_renders_object_destructuring_for_loop_pattern() {
        let source_text = "agent analyzer for {id,name,} in agent.alpha.participants {instruction:\"hello\" output{value:string}}\n";
        let expected_output =
            "agent analyzer for { id, name } in agent.alpha.participants {\n    instruction: \"hello\"\n    output {\n        value: string\n    }\n}\n";

        let formatted_source = format_workflow_source(source_text).expect("workflow should format successfully");

        assert_eq!(formatted_source, expected_output);
    }

    #[test]
    fn formatter_renders_mcp_tool_batch_imports() {
        let source_text =
            "from mcp.local.tool{bindings{project_id:1 task_id:2}tool create_sorting_task{bindings{title:\"Sort\"}}tool assign_task}\n";
        let expected_output = "from mcp.local.tool {\n    bindings {\n        project_id: 1\n        task_id: 2\n    }\n\n    tool create_sorting_task {\n        bindings {\n            title: \"Sort\"\n        }\n    }\n    tool assign_task\n}\n";

        let formatted_source = format_workflow_source(source_text).expect("batch import workflow should format successfully");

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
