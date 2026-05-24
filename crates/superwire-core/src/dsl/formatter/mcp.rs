use std::fmt::Write;

use crate::dsl::ast::{
    DeclarationKeyword, ImportKeyword, McpBatchImportDeclaration, McpCall, McpImportKind, McpImportPropertyName,
    McpPromptBatchImportDeclaration, McpPromptBatchImportItem, McpPromptImportDeclaration, McpResourceBatchImportDeclaration,
    McpResourceBatchImportItem, McpResourceImportDeclaration, McpToolBatchImportDeclaration, McpToolBatchImportItem,
    McpToolBatchImportPropertyName, ObjectField, ToolPropertyName,
};

use super::DslFormatter;

impl McpToolBatchImportDeclaration {
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter) {
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
                McpToolBatchImportPropertyName::MaxCalls.definition().name
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
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter) {
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
            formatter.push_declaration_block_start(McpToolBatchImportPropertyName::Bindings.definition().name);

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
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter) {
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
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter) {
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

impl McpResourceBatchImportItem {
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter) {
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

impl McpPromptBatchImportItem {
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter) {
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

impl McpToolBatchImportItem {
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter) {
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

impl McpResourceImportDeclaration {
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter) {
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
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter) {
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

impl McpCall {
    pub(super) fn push_to_formatter(&self, formatter: &mut DslFormatter) {
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
