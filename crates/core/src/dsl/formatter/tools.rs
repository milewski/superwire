use crate::dsl::ast::{DeclarationKeyword, ImportKeyword, ToolCall, ToolCallPropertyName, ToolDeclaration, ToolPropertyName, ToolSource};

use super::wrapping::render_plain_string_literal;
use super::DslFormatter;

impl ToolDeclaration {
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter) {
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
        let Some(ToolSource::Mcp(mcp_tool_source)) = &self.source else {
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

impl ToolCall {
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter) {
        formatter.output.push_str("call ");
        self.callee.push_to_formatter(formatter);

        if !self.has_body() {
            return;
        }

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

    fn has_body(&self) -> bool {
        !self.input_fields.is_empty() || !self.binding_fields.is_empty() || self.max_calls.is_some()
    }

    pub(super) fn push_agent_binding_to_formatter(&self, formatter: &mut DslFormatter) {
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
